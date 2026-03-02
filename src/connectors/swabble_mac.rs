// Swabble iOS / macOS Swift application backend bridge
#[cfg(feature = "swabble")]
pub struct SwabbleMacClient {}

#[cfg(feature = "swabble")]
impl Default for SwabbleMacClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "swabble")]
impl SwabbleMacClient {
    pub fn new() -> Self {
        Self {}
    }
    
    pub fn native_transcribe(&self) {
        println!("Swabble: Executing TranscribeCommand.swift bindings.");
    }
}
