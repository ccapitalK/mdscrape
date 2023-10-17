use lazy_static::*;

pub const user_agent: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/118.0";

lazy_static! {
    pub static ref CLIENT: reqwest::Client = reqwest::ClientBuilder::new().user_agent(user_agent).build().unwrap();
}
