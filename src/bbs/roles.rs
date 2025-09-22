/// Role / privilege level constants
pub const LEVEL_USER: u8 = 1;
pub const LEVEL_MODERATOR: u8 = 5;
pub const LEVEL_SYSOP: u8 = 10;

pub fn role_name(level: u8) -> &'static str {
    match level {
        LEVEL_SYSOP => "Sysop",
        LEVEL_MODERATOR => "Moderator",
        _ => "User",
    }
}