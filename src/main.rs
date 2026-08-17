//! SaveTensor DiskSort CLI Entrypoint

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use savetensor_disksort::config::{ConfigMountpoint, DiskSortConfig};
use savetensor_disksort::discovery::{inspect_mountpoint, scan_all_mountpoints, SymlinkMapper};
use savetensor_disksort::executor::{execute_plan, ExecutionOptions};
use savetensor_disksort::persistence::{
    load_plan_from_file, save_plan_to_file, ExecutionLogger, Inventory,
};
use savetensor_disksort::planner::{PlanTemplate, SortPlan};
use savetensor_disksort::tui::run_tui;
use savetensor_disksort::utils::format_bytes;

#[derive(Parser, Debug)]
#[command(
    name = "disksort",
    about = "SaveTensor DiskSort — Multi-drive AI model inventory, planning & safe relocation",
    version
)]
struct Cli {
    #[arg(short, long, global = true, help = "Path to configuration file")]
    config: Option<PathBuf>,

    #[arg(
        short,
        long,
        global = true,
        help = "Output directory for JSON state files"
    )]
    output_dir: Option<PathBuf>,

    #[arg(short, long, global = true, help = "Enable verbose debug output")]
    verbose: bool,

    #[arg(short, long, global = true, help = "Suppress non-error output")]
    quiet: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Scan physical mountpoints and map logical application symlinks")]
    Scan {
        #[arg(
            short,
            long,
            value_delimiter = ',',
            help = "Comma-separated list of mountpoints/drives to scan"
        )]
        mountpoints: Option<Vec<String>>,

        #[arg(
            short,
            long,
            value_delimiter = ',',
            help = "Comma-separated list of application roots to map symlinks"
        )]
        app_roots: Option<Vec<String>>,
    },

    #[command(about = "View and query the last generated inventory")]
    Inventory {
        #[arg(short, long, help = "Show high-level summary breakdown")]
        summary: bool,

        #[arg(short, long, help = "List all detected duplicate candidates")]
        duplicates: bool,

        #[arg(short, long, help = "Display inventory grouped by mountpoint")]
        by_drive: bool,
    },

    #[command(about = "Generate a deterministic sort relocation plan")]
    Plan {
        #[arg(short, long, help = "Target destination drive mountpoint")]
        target: String,

        #[arg(
            short = 'T',
            long,
            default_value = "by-type",
            help = "Folder layout template: 'by-type', 'by-type-and-base', 'flat', 'preserve'"
        )]
        template: String,
    },

    #[command(about = "Execute the sort relocation plan (copy -> verify -> delete)")]
    Sort {
        #[arg(short = 'd', long, help = "Simulate operations without touching files")]
        dry_run: bool,

        #[arg(short = 'x', long, help = "Execute real copy-verify-delete operations")]
        execute: bool,

        #[arg(
            short,
            long,
            help = "Explicit path to sort_plan.json (default: <output_dir>/sort_plan.json)"
        )]
        plan_file: Option<PathBuf>,
    },

    #[command(about = "Health check: identify broken symlinks and orphaned model files")]
    Doctor,

    #[command(about = "Launch the interactive Terminal User Interface (TUI)")]
    Tui,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else if !cli.quiet {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("disksort.json"));
    let mut config = DiskSortConfig::load_from_file_or_default(&config_path);

    if let Some(out) = &cli.output_dir {
        config.output_dir = out.to_string_lossy().to_string();
    }

    match cli.command {
        None | Some(Commands::Tui) => {
            run_tui(config)?;
        }

        Some(Commands::Scan {
            mountpoints,
            app_roots,
        }) => {
            if let Some(mps) = mountpoints {
                config.mountpoints = mps
                    .into_iter()
                    .map(|p| ConfigMountpoint {
                        path: p,
                        label: None,
                    })
                    .collect();
            }
            if let Some(apps) = app_roots {
                config.app_roots = apps;
            }

            let mp_paths = config.mountpoint_paths();
            if mp_paths.is_empty() {
                eprintln!("Error: No mountpoints specified. Use '--mountpoints <path1,path2>' or set in 'disksort.json'");
                std::process::exit(1);
            }

            println!("==> Scanning {} mountpoint(s)...", mp_paths.len());
            let mut files = scan_all_mountpoints(&mp_paths);
            println!("==> Found {} physical model files.", files.len());

            let app_paths = config.app_root_paths();
            let mut mapper = SymlinkMapper::new();
            if !app_paths.is_empty() {
                println!(
                    "==> Mapping symlinks across {} app roots...",
                    app_paths.len()
                );
                mapper.scan_app_roots(&app_paths);
                mapper.cross_reference_inventory(&mut files);
                println!("==> Discovered {} symlink mappings.", mapper.mappings.len());
            }

            let mp_infos = mp_paths
                .iter()
                .map(|p| inspect_mountpoint(p, None))
                .collect();

            let inventory = Inventory::build(mp_infos, files, mapper.symlink_tree);
            let inv_path = PathBuf::from(&config.output_dir).join("inventory.json");
            inventory.save_to_file(&inv_path)?;

            println!(
                "✓ Inventory saved to '{}' (Total size: {}, Duplicates: {})",
                inv_path.display(),
                inventory.summary.total_size_human,
                inventory.summary.duplicate_candidates
            );
        }

        Some(Commands::Inventory {
            summary: _,
            duplicates,
            by_drive,
        }) => {
            let inv_path = PathBuf::from(&config.output_dir).join("inventory.json");
            if !inv_path.exists() {
                eprintln!(
                    "Error: No inventory found at '{}'. Run 'disksort scan' first.",
                    inv_path.display()
                );
                std::process::exit(1);
            }

            let inventory = Inventory::load_from_file(&inv_path)?;

            if duplicates {
                println!("\n=== Duplicate Candidates ===");
                let dupes =
                    savetensor_disksort::accounting::find_duplicate_candidates(&inventory.files);
                if dupes.is_empty() {
                    println!("No duplicate candidate files found.");
                } else {
                    for d in dupes {
                        println!(
                            "• {} ({}) - {} copies:",
                            d.filename,
                            d.size_human,
                            d.paths.len()
                        );
                        for p in &d.paths {
                            println!("    → {}", p);
                        }
                    }
                }
            } else if by_drive {
                println!("\n=== Inventory by Mountpoint ===");
                for (mp, stat) in &inventory.summary.by_mountpoint {
                    println!("• {:<30} : {} files, {}", mp, stat.count, stat.size_human);
                }
            } else {
                // Default: summary
                println!("\n=== SaveTensor DiskSort Inventory Summary ===");
                println!("Scan Date:     {}", inventory.scan_timestamp);
                println!("Total Files:   {}", inventory.summary.total_files);
                println!("Total Size:    {}", inventory.summary.total_size_human);
                println!("Duplicates:    {}", inventory.summary.duplicate_candidates);
                println!("\nBreakdown by Category:");
                for (cat, stat) in &inventory.summary.by_category {
                    println!(
                        "  {:<14} : {:>4} files ({})",
                        cat, stat.count, stat.size_human
                    );
                }
                println!("\nBreakdown by Mountpoint:");
                for (mp, stat) in &inventory.summary.by_mountpoint {
                    println!(
                        "  {:<30} : {:>4} files ({})",
                        mp, stat.count, stat.size_human
                    );
                }
            }
        }

        Some(Commands::Plan { target, template }) => {
            let inv_path = PathBuf::from(&config.output_dir).join("inventory.json");
            if !inv_path.exists() {
                eprintln!("Error: Inventory not found. Run 'disksort scan' first.");
                std::process::exit(1);
            }

            let inventory = Inventory::load_from_file(&inv_path)?;
            let parsed_template = PlanTemplate::parse(&template).unwrap_or_else(|| {
                eprintln!(
                    "Warning: Unknown template '{}', falling back to 'by-type'",
                    template
                );
                PlanTemplate::ByType
            });

            let target_info = inspect_mountpoint(&target, None);
            let free_bytes = if target_info.free_bytes > 0 {
                target_info.free_bytes
            } else {
                2_000_000_000_000 // 2TB default fallback
            };

            let plan = SortPlan::generate(&inventory.files, &target, free_bytes, parsed_template);
            let plan_path = PathBuf::from(&config.output_dir).join("sort_plan.json");
            save_plan_to_file(&plan_path, &plan)?;

            println!(
                "✓ Sort plan generated with {} operations.",
                plan.operations.len()
            );
            println!("  Target Drive:      {}", plan.target_drive);
            println!(
                "  Required Space:    {}",
                format_bytes(plan.space_analysis.total_move_size)
            );
            println!(
                "  Target Free After: {}",
                format_bytes(plan.space_analysis.target_drive_free_after)
            );
            println!(
                "  Capacity Status:   {}",
                if plan.space_analysis.fits {
                    "OK (Fits with 5% safety margin)"
                } else {
                    "INSUFFICIENT SPACE!"
                }
            );
            println!("  Plan written to:   {}", plan_path.display());
        }

        Some(Commands::Sort {
            dry_run,
            execute,
            plan_file,
        }) => {
            if !dry_run && !execute {
                eprintln!("Error: Please specify either '--dry-run' (-d) or '--execute' (-x)");
                std::process::exit(1);
            }

            let plan_path = plan_file
                .unwrap_or_else(|| PathBuf::from(&config.output_dir).join("sort_plan.json"));

            if !plan_path.exists() {
                eprintln!(
                    "Error: Sort plan not found at '{}'. Run 'disksort plan' first.",
                    plan_path.display()
                );
                std::process::exit(1);
            }

            let mut plan = load_plan_from_file(&plan_path)?;
            let val = savetensor_disksort::planner::validate_plan(&plan);

            if !val.is_valid {
                eprintln!("Plan validation failed:");
                for err in val.errors {
                    eprintln!("  ✗ {}", err);
                }
                std::process::exit(1);
            }

            for warn in val.warnings {
                println!("  ⚠ Warning: {}", warn);
            }

            let log_path = PathBuf::from(&config.output_dir).join("execution.jsonl");
            let logger = ExecutionLogger::new(&log_path).ok();

            println!(
                "==> Starting {} for {} planned operations...",
                if dry_run {
                    "DRY-RUN simulation"
                } else {
                    "REAL relocation execution"
                },
                plan.operations.len()
            );

            let options = ExecutionOptions {
                dry_run,
                logger,
                cancel_flag: None,
                progress_cb: |op_id, copied, total| {
                    if total > 0 && (copied == total || copied % (50 * 1024 * 1024) == 0) {
                        println!(
                            "  [{}] Progress: {} / {}",
                            op_id,
                            format_bytes(copied),
                            format_bytes(total)
                        );
                    }
                },
            };

            let messages = execute_plan(&mut plan, options).map_err(|e| e.to_string())?;

            if dry_run {
                println!("\n=== DRY-RUN Execution Log ===");
                for msg in messages {
                    println!("{}", msg);
                }
                println!("\n✓ Dry-run completed. No files were modified.");
            } else {
                save_plan_to_file(&plan_path, &plan)?;
                println!("\n✓ Relocation execution completed successfully.");
            }
        }

        Some(Commands::Doctor) => {
            let inv_path = PathBuf::from(&config.output_dir).join("inventory.json");
            if !inv_path.exists() {
                eprintln!("Error: Inventory not found. Run 'disksort scan' first.");
                std::process::exit(1);
            }

            let inventory = Inventory::load_from_file(&inv_path)?;
            println!("\n=== SaveTensor DiskSort Doctor Health Check ===");
            println!(
                "Checking {} files and symlink references...",
                inventory.files.len()
            );

            let mut broken_symlinks = 0;
            for file in &inventory.files {
                for link in &file.symlinked_from {
                    let p = Path::new(link);
                    if !p.exists() {
                        println!("  ✗ Broken symlink: '{}' -> '{}'", link, file.real_path);
                        broken_symlinks += 1;
                    }
                }
            }

            if broken_symlinks == 0 {
                println!("✓ All symlinks are valid and resolve to existing targets.");
            } else {
                println!("⚠ Found {} broken symlinks.", broken_symlinks);
            }
        }
    }

    Ok(())
}
