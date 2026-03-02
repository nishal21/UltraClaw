#[cfg(feature = "phonecontrol")]
pub struct PhoneController {}

#[cfg(feature = "phonecontrol")]
impl Default for PhoneController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "phonecontrol")]
impl PhoneController {
    pub fn new() -> Self {
        Self {}
    }

    pub fn make_call(&self, number: &str) {
        println!("Phone Controller: Instructing physical device to call {}", number);
    }
}
