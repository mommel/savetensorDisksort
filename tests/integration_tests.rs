use std::fs;
use tempfile::tempdir;

use savetensor_disksort::accounting::find_duplicate_candidates;
use savetensor_disksort::discovery::{
    inspect_mountpoint, scan_all_mountpoints, Category, SymlinkMapper,
};
use savetensor_disksort::executor::{
    analyze_operation_recovery, execute_plan, execute_recovery_action, ExecutionOptions,
};
use savetensor_disksort::persistence::{
    read_execution_log, save_plan_to_file, ExecutionLogger, Inventory,
};
use savetensor_disksort::planner::{validate_plan, PlanTemplate, SortPlan};
use savetensor_disksort::utils::{canonicalize_lossy, hash_file, normalize_path};

#[test]
fn test_integration_discovery_and_symlinks() {
    let root = tempdir().unwrap();

    // Setup simulated Drive A and Drive B
    let drive_a = root.path().join("mnt").join("drive_a");
    let drive_b = root.path().join("mnt").join("drive_b");
    let app_root = root.path().join("home").join("user").join("ComfyUI");

    let sd_dir = drive_a.join("models").join("checkpoints");
    let lora_dir = drive_b.join("models").join("loras");
    let app_ckpt_dir = app_root.join("models").join("checkpoints");

    fs::create_dir_all(&sd_dir).unwrap();
    fs::create_dir_all(&lora_dir).unwrap();
    fs::create_dir_all(&app_ckpt_dir).unwrap();

    let model1_path = sd_dir.join("v1-5-pruned.safetensors");
    let lora1_path = lora_dir.join("detail.safetensors");

    let model_bytes = b"SD1.5 Safetensors model weight binary content 123456789";
    let lora_bytes = b"LoRA adapter weight content";

    fs::write(&model1_path, model_bytes).unwrap();
    fs::write(&lora1_path, lora_bytes).unwrap();

    // Create symlink from ComfyUI to Drive A model
    let symlink_path = app_ckpt_dir.join("v1-5-pruned.safetensors");
    let _ = savetensor_disksort::executor::create_symlink(&model1_path, &symlink_path);

    // Run Discovery
    let mountpoints = vec![drive_a.clone(), drive_b.clone()];
    let mut files = scan_all_mountpoints(&mountpoints);

    assert_eq!(files.len(), 2);
    let f1 = files
        .iter()
        .find(|f| f.filename == "v1-5-pruned.safetensors")
        .unwrap();
    assert_eq!(f1.category, Category::Checkpoint);

    let f2 = files
        .iter()
        .find(|f| f.filename == "detail.safetensors")
        .unwrap();
    assert_eq!(f2.category, Category::Lora);

    // Run Symlink Mapping
    let mut mapper = SymlinkMapper::new();
    mapper.scan_app_roots(std::slice::from_ref(&app_root));
    mapper.cross_reference_inventory(&mut files);

    let updated_f1 = files
        .iter()
        .find(|f| f.filename == "v1-5-pruned.safetensors")
        .unwrap();
    if symlink_path.exists() {
        assert_eq!(updated_f1.symlinked_from.len(), 1);
    }

    // Build and verify inventory
    let mp_infos = vec![
        inspect_mountpoint(&drive_a, Some("DriveA")),
        inspect_mountpoint(&drive_b, Some("DriveB")),
    ];

    let inventory = Inventory::build(mp_infos, files, mapper.symlink_tree);
    assert_eq!(inventory.summary.total_files, 2);
    assert_eq!(
        inventory.summary.total_size_bytes,
        (model_bytes.len() + lora_bytes.len()) as u64
    );

    let folder_tree = &inventory.folder_tree;
    assert_eq!(folder_tree.len(), 2);
}

#[test]
fn test_integration_full_sort_lifecycle() {
    let root = tempdir().unwrap();

    // Setup source drives and target drive
    let drive_nvme = root.path().join("mnt").join("nvme");
    let drive_usbc = root.path().join("mnt").join("usbc");
    let app_dir = root.path().join("app").join("models");

    let src_dir = drive_nvme.join("models").join("checkpoints");
    let app_ckpt = app_dir.join("sd15.safetensors");

    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&drive_usbc).unwrap();
    fs::create_dir_all(&app_dir).unwrap();

    let model_src = src_dir.join("v1-5-pruned.safetensors");
    let model_data = b"Precious model weights that must NEVER be lost or corrupted";
    fs::write(&model_src, model_data).unwrap();
    let original_hash = hash_file(&model_src).unwrap();

    // Setup application symlink
    let _ = savetensor_disksort::executor::create_symlink(&model_src, &app_ckpt);

    // 1. Scan
    let mountpoints = vec![drive_nvme.clone(), drive_usbc.clone()];
    let mut files = scan_all_mountpoints(&mountpoints);
    let mut mapper = SymlinkMapper::new();
    mapper.scan_app_roots(&[root.path().join("app")]);
    mapper.cross_reference_inventory(&mut files);

    let out_dir = root.path().join("disksort_data");
    fs::create_dir_all(&out_dir).unwrap();

    let inv = Inventory::build(vec![], files.clone(), mapper.symlink_tree);
    let inv_path = out_dir.join("inventory.json");
    inv.save_to_file(&inv_path).unwrap();

    // 2. Plan
    let target_drive_str = normalize_path(&drive_usbc);
    let mut plan = SortPlan::generate(
        &files,
        &target_drive_str,
        500_000_000_000,
        PlanTemplate::ByType,
    );
    let plan_path = out_dir.join("sort_plan.json");
    save_plan_to_file(&plan_path, &plan).unwrap();

    let validation = validate_plan(&plan);
    assert!(validation.is_valid);
    assert_eq!(plan.operations.len(), 1);

    let expected_dest = drive_usbc
        .join("models")
        .join("checkpoints")
        .join("v1-5-pruned.safetensors");
    assert_eq!(
        normalize_path(canonicalize_lossy(&expected_dest)),
        normalize_path(canonicalize_lossy(&plan.operations[0].destination))
    );

    // 3. Dry-Run Execution
    let dry_run_options = ExecutionOptions {
        dry_run: true,
        logger: None,
        cancel_flag: None,
        progress_cb: |_, _, _| {},
    };

    let dry_run_msgs = execute_plan(&mut plan, dry_run_options).unwrap();
    assert!(!dry_run_msgs.is_empty());
    assert!(model_src.exists(), "Dry run must NOT delete source file");
    assert!(
        !expected_dest.exists(),
        "Dry run must NOT create destination file"
    );

    // 4. Real Execution
    let log_path = out_dir.join("execution.jsonl");
    let logger = ExecutionLogger::new(&log_path).unwrap();

    let real_options = ExecutionOptions {
        dry_run: false,
        logger: Some(logger),
        cancel_flag: None,
        progress_cb: |_, _, _| {},
    };

    let real_res = execute_plan(&mut plan, real_options);
    assert!(real_res.is_ok());

    // 5. Verification of post-execution state
    assert!(
        !model_src.exists(),
        "Original source file must be safely removed after verification"
    );
    assert!(expected_dest.exists(), "Destination file must exist");

    // Verify destination bit-for-bit
    let dest_hash = hash_file(&expected_dest).unwrap();
    assert_eq!(
        dest_hash, original_hash,
        "Destination BLAKE3 hash must match original bit-for-bit"
    );

    // Verify symlink was redirected if supported
    if app_ckpt.exists() {
        let dest_canon = normalize_path(canonicalize_lossy(&expected_dest));
        let link_target = fs::read_link(&app_ckpt).unwrap();
        let target_canon = normalize_path(canonicalize_lossy(&link_target));
        assert_eq!(
            target_canon, dest_canon,
            "Symlink must point to new destination"
        );
    }

    // Verify execution log
    let events = read_execution_log(&log_path).unwrap();
    assert!(!events.is_empty());
    assert!(events.iter().any(|e| e.phase == "copy_start"));
    assert!(events.iter().any(|e| e.phase == "verify_ok"));
    assert!(events.iter().any(|e| e.phase == "delete_original"));
    assert!(events.iter().any(|e| e.phase == "complete"));
}

#[test]
fn test_integration_duplicate_detection() {
    let root = tempdir().unwrap();

    let drive1 = root.path().join("d1");
    let drive2 = root.path().join("d2");

    fs::create_dir_all(&drive1).unwrap();
    fs::create_dir_all(&drive2).unwrap();

    let f1 = drive1.join("sd_xl_base.safetensors");
    let f2 = drive2.join("sd_xl_base.safetensors");

    let content = vec![77u8; 500_000]; // 500KB
    fs::write(&f1, &content).unwrap();
    fs::write(&f2, &content).unwrap();

    let files = scan_all_mountpoints(&[drive1, drive2]);
    let dupes = find_duplicate_candidates(&files);

    assert_eq!(dupes.len(), 1);
    assert_eq!(dupes[0].filename, "sd_xl_base.safetensors");
    assert_eq!(dupes[0].paths.len(), 2);
}

#[test]
fn test_integration_crash_recovery() {
    let root = tempdir().unwrap();
    let out_dir = root.path().join("disksort_data");
    fs::create_dir_all(&out_dir).unwrap();

    let log_path = out_dir.join("execution.jsonl");
    let logger = ExecutionLogger::new(&log_path).unwrap();

    let src = root.path().join("source.safetensors");
    let dst = root.path().join("dest.safetensors");
    fs::write(&src, b"recovery test content").unwrap();

    logger.log_copy_start("op-0042", &normalize_path(&src), &normalize_path(&dst));
    // Simulate crash right after copy start (destination incomplete)
    let tmp_dst = dst.with_extension("safetensors.disksort_tmp");
    fs::write(&tmp_dst, b"partial incomplete data").unwrap();

    let events = read_execution_log(&log_path).unwrap();
    let recovery_action = analyze_operation_recovery(&events);

    match recovery_action {
        savetensor_disksort::executor::RecoveryAction::CleanIncompleteDest {
            dst: ref target_dst,
        } => {
            assert_eq!(target_dst, &normalize_path(&dst));
            let res = execute_recovery_action(&recovery_action);
            assert!(res.is_ok());
            assert!(!tmp_dst.exists(), "Incomplete temp file should be removed");
            assert!(src.exists(), "Source file should remain intact");
        }
        _ => panic!("Expected CleanIncompleteDest action"),
    }
}
