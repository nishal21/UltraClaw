// Concrete Android Native Kotlin configuration payload
#[cfg(feature = "android")]
pub struct AndroidAppClient {
    application_id: String,
}

#[cfg(feature = "android")]
impl AndroidAppClient {
    pub fn new() -> Self {
        Self { application_id: "com.ultraclaw.android".into() }
    }
}
