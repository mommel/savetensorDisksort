pub mod duplicates;
pub mod size_tree;

pub use duplicates::{find_duplicate_candidates, DuplicateGroup};
pub use size_tree::{build_folder_tree, FolderNode};
