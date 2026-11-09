pub mod entropy;
pub mod sha256;

pub fn init() {
    entropy::init();
}
