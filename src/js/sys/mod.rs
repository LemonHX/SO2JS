pub trait Sys {
    fn dir_concat(&self, dir: &str, file: &str) -> alloc::string::String;
    fn file_get_parent_dir(&self, path: &str) -> alloc::string::String;
    fn file_path_canonicalize(&self, path: &str) -> alloc::string::String;
    fn read_file(&self, path: &str) -> Option<alloc::vec::Vec<u8>>;
    fn write_file(&self, path: &str, data: &[u8]) -> bool;
    fn current_time_millis(&self) -> f64;
}
