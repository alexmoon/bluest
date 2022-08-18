pub struct Adapter;

impl Adapter {
    /// Creates an interface to the default Bluetooth adapter for the system
    pub async fn default() -> Option<Self> {
        None
    }
}
