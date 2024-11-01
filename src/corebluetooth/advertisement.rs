use std::error::Error;
use std::time::Duration;
use async_trait::async_trait;

pub struct AdvertisementImpl {
    company_id: u16,
}

impl AdvertisementImpl {
    pub fn new(company_id: u16) -> Self {
        AdvertisementImpl { company_id }
    }
}

#[async_trait]
impl Advertisement for AdvertisementImpl {
    async fn advertise(&self, data: &Vec<u8>, advertise_duration: Option<Duration>) -> Result<(), Box<dyn Error>> {
        println!("CoreBluetooth advertisement started with company ID: {:X}.", self.company_id);

        if let Some(duration) = advertise_duration {
            tokio::time::sleep(duration).await;
            self.stop()?;
            println!("CoreBluetooth advertisement stopped after {:?}", duration);
        }

        Ok(())
    }

    fn stop(&self) -> Result<(), Box<dyn Error>> {
        println!("CoreBluetooth advertisement manually stopped.");
        Ok(())
    }
}
