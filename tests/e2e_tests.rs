use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

use savetensor_disksort::config::{ConfigMountpoint, DiskSortConfig};
use savetensor_disksort::discovery::{inspect_mountpoint, scan_all_mountpoints, SymlinkMapper};
use savetensor_disksort::executor::{execute_plan, ExecutionOptions};
use savetensor_disksort::persistence::{
    load_plan_from_file, save_plan_to_file, ExecutionLogger, Inventory,
};
use savetensor_disksort::planner::{validate_plan, PlanTemplate, SortPlan};
use savetensor_disksort::utils::{hash_file, normalize_path};

#[test]
fn test_e2e_full_workflow() {
    let root = tempdir().unwrap();

    // Create 3 storage partitions / folders
    let drive1 = root.path().join("drives").join("drive1");
    let drive2 = root.path().join("drives").join("drive2");
    let drive_target = root.path().join("drives").join("drive_target");
    let app_root = root.path().join("apps").join("webui");

    let ckpt1 = drive1
        .join("models")
        .join("checkpoints")
        .join("model_a.safetensors");
    let ckpt2 = drive2
        .join("models")
        .join("checkpoints")
        .join("model_b.safetensors");
    let lora1 = drive1
        .join("models")
        .join("loras")
        .join("style_a.safetensors");

    fs::create_dir_all(ckpt1.parent().unwrap()).unwrap();
    fs::create_dir_all(ckpt2.parent().unwrap()).unwrap();
    fs::create_dir_all(lora1.parent().unwrap()).unwrap();
    fs::create_dir_all(&drive_target).unwrap();
    fs::create_dir_all(&app_root).unwrap();

    let data_a = b"Model A Safetensors Checkpoint Weight Data";
    let data_b = b"Model B Safetensors Checkpoint Weight Data";
    let data_lora = b"LoRA Adapter Weights";

    fs::write(&ckpt1, data_a).unwrap();
    fs::write(&ckpt2, data_b).unwrap();
    fs::write(&lora1, data_lora).unwrap();

    let hash_a = hash_file(&ckpt1).unwrap();
    let hash_b = hash_file(&ckpt2).unwrap();
    let hash_lora = hash_file(&lora1).unwrap();

    // Create app symlinks
    let link_a = app_root.join("model_a.safetensors");
    let _ = savetensor_disksort::executor::create_symlink(&ckpt1, &link_a);

    let config = DiskSortConfig {
        mountpoints: vec![
            ConfigMountpoint {
                path: normalize_path(&drive1),
                label: Some("Drive 1".into()),
            },
            ConfigMountpoint {
                path: normalize_path(&drive2),
                label: Some("Drive 2".into()),
            },
        ],
        app_roots: vec![normalize_path(&app_root)],
        output_dir: normalize_path(root.path().join("data")),
        ..Default::default()
    };

    // Step 1: Scan
    let mp_paths = config.mountpoint_paths();
    let mut files = scan_all_mountpoints(&mp_paths);
    assert_eq!(files.len(), 3);

    let mut mapper = SymlinkMapper::new();
    mapper.scan_app_roots(&config.app_root_paths());
    mapper.cross_reference_inventory(&mut files);

    let mp_infos = mp_paths
        .iter()
        .map(|p| inspect_mountpoint(p, None))
        .collect();
    let inv = Inventory::build(mp_infos, files.clone(), mapper.symlink_tree);
    let inv_path = PathBuf::from(&config.output_dir).join("inventory.json");
    inv.save_to_file(&inv_path).unwrap();

    assert_eq!(inv.summary.total_files, 3);
    assert_eq!(
        inv.summary.total_size_bytes,
        (data_a.len() + data_b.len() + data_lora.len()) as u64
    );

    // Step 2: Plan
    let target_str = normalize_path(&drive_target);
    let mut plan = SortPlan::generate(&files, &target_str, 100_000_000_000, PlanTemplate::ByType);
    let plan_path = PathBuf::from(&config.output_dir).join("sort_plan.json");
    save_plan_to_file(&plan_path, &plan).unwrap();

    assert_eq!(plan.operations.len(), 3);
    assert!(plan.space_analysis.fits);

    // Step 3: Dry-run
    let dry_run_options = ExecutionOptions {
        dry_run: true,
        logger: None,
        cancel_flag: None,
        progress_cb: |_, _, _| {},
    };
    let dry_run_msgs = execute_plan(&mut plan, dry_run_options).unwrap();
    assert_eq!(dry_run_msgs.len(), 3);
    assert!(ckpt1.exists());
    assert!(ckpt2.exists());
    assert!(lora1.exists());

    // Step 4: Real Relocation Execution
    let log_path = PathBuf::from(&config.output_dir).join("execution.jsonl");
    let logger = ExecutionLogger::new(&log_path).unwrap();

    let exec_options = ExecutionOptions {
        dry_run: false,
        logger: Some(logger),
        cancel_flag: None,
        progress_cb: |_, _, _| {},
    };
    let _ = execute_plan(&mut plan, exec_options).unwrap();

    // Verify originals deleted and destinations exist with matching hashes
    assert!(!ckpt1.exists());
    assert!(!ckpt2.exists());
    assert!(!lora1.exists());

    let dest_a = drive_target
        .join("models")
        .join("checkpoints")
        .join("model_a.safetensors");
    let dest_b = drive_target
        .join("models")
        .join("checkpoints")
        .join("model_b.safetensors");
    let dest_lora = drive_target
        .join("models")
        .join("loras")
        .join("style_a.safetensors");

    assert!(dest_a.exists());
    assert!(dest_b.exists());
    assert!(dest_lora.exists());

    assert_eq!(hash_file(&dest_a).unwrap(), hash_a);
    assert_eq!(hash_file(&dest_b).unwrap(), hash_b);
    assert_eq!(hash_file(&dest_lora).unwrap(), hash_lora);
}

#[test]
fn test_e2e_insufficient_space_validation() {
    let root = tempdir().unwrap();
    let src = root.path().join("model.safetensors");
    fs::write(&src, b"Huge model content").unwrap();

    let files = scan_all_mountpoints(&[root.path().to_path_buf()]);
    // Simulate only 5 bytes free space on target
    let plan = SortPlan::generate(&files, "/mnt/target", 5, PlanTemplate::ByType);

    assert!(!plan.space_analysis.fits);
    let val = validate_plan(&plan);
    assert!(!val.is_valid);
    assert!(val
        .errors
        .iter()
        .any(|e| e.contains("Insufficient disk space")));
}

#[test]
fn test_e2e_user_modified_json_plan() {
    let root = tempdir().unwrap();
    let src_file = root.path().join("original.safetensors");
    fs::write(&src_file, b"User custom plan test content").unwrap();
    let original_hash = hash_file(&src_file).unwrap();

    let custom_dest = root
        .path()
        .join("custom_folder")
        .join("renamed.safetensors");

    let files = scan_all_mountpoints(&[root.path().to_path_buf()]);
    let mut plan = SortPlan::generate(&files, "/target", 1_000_000_000, PlanTemplate::ByType);

    // Modify operation manually as user would in JSON
    plan.operations[0].destination = normalize_path(&custom_dest);

    let plan_file = root.path().join("custom_sort_plan.json");
    save_plan_to_file(&plan_file, &plan).unwrap();

    // Reload and execute
    let mut reloaded_plan = load_plan_from_file(&plan_file).unwrap();
    assert_eq!(
        reloaded_plan.operations[0].destination,
        normalize_path(&custom_dest)
    );

    let options = ExecutionOptions {
        dry_run: false,
        logger: None,
        cancel_flag: None,
        progress_cb: |_, _, _| {},
    };

    let _ = execute_plan(&mut reloaded_plan, options).unwrap();

    assert!(!src_file.exists());
    assert!(custom_dest.exists());
    assert_eq!(hash_file(&custom_dest).unwrap(), original_hash);
}

#[test]
fn test_e2e_symlink_cycles_safe() {
    let root = tempdir().unwrap();
    let dir_a = root.path().join("dir_a");
    let dir_b = root.path().join("dir_b");

    fs::create_dir_all(&dir_a).unwrap();
    fs::create_dir_all(&dir_b).unwrap();

    // Create cyclic symlinks A/link_b -> B and B/link_a -> A
    let link_to_b = dir_a.join("link_b");
    let link_to_a = dir_b.join("link_a");

    let res_b = savetensor_disksort::executor::create_symlink(&dir_b, &link_to_b);
    let res_a = savetensor_disksort::executor::create_symlink(&dir_a, &link_to_a);

    let mut mapper = SymlinkMapper::new();
    // Scan app roots must terminate and detect cycles without infinite recursion
    mapper.scan_app_roots(&[dir_a, dir_b]);
    if res_b.is_ok() || res_a.is_ok() {
        assert!(!mapper.mappings.is_empty());
    }
}
