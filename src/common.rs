pub type OpaqueError = Box<dyn std::error::Error>;
pub type OpaqueResult<T> = Result<T, OpaqueError>;

pub fn escape_path_string(s: String) -> String {
    s.chars().map(|x| if x == '/' { '-' } else { x }).collect()
}
