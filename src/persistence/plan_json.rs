//! Serialization and deserialization for `sort_plan.json`.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::Path;

use crate::planner::SortPlan;

/// Save a `SortPlan` to disk formatted with standard indentation.
pub fn save_plan_to_file<P: AsRef<Path>>(path: P, plan: &SortPlan) -> std::io::Result<()> {
    let p = path.as_ref();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(p)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, plan)?;
    Ok(())
}

/// Load a `SortPlan` from a JSON file.
pub fn load_plan_from_file<P: AsRef<Path>>(path: P) -> std::io::Result<SortPlan> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let plan = serde_json::from_reader(reader)?;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_plan_save_load() {
        let plan = SortPlan::generate(&[], "/mnt/target", 1_000_000_000, crate::planner::PlanTemplate::ByType);
        let file = NamedTempFile::new().unwrap();

        save_plan_to_file(file.path(), &plan).unwrap();
        let loaded = load_plan_from_file(file.path()).unwrap();

        assert_eq!(plan.version, loaded.version);
        assert_eq!(plan.target_drive, loaded.target_drive);
    }
}
