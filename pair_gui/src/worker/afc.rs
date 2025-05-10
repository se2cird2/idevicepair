use std::path::PathBuf;
use idevice::{
    afc::{opcode::AfcFopenMode, AfcClient},
    house_arrest::HouseArrestClient,
    IdeviceService,
};

use crate::util::get_provider;

pub async fn list_files(
    udid: &str,
    path: &str,
    container: Option<&str>,
    documents: Option<&str>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let provider = get_provider(Some(udid), None, None, "afc-gui").await?;

    let mut afc_client = if let Some(bundle_id) = container {
        let h = HouseArrestClient::connect(&*provider).await?;
        h.vend_container(bundle_id).await?
    } else if let Some(bundle_id) = documents {
        let h = HouseArrestClient::connect(&*provider).await?;
        h.vend_documents(bundle_id).await?
    } else {
        AfcClient::connect(&*provider).await?
    };

    let list = afc_client.list_dir(path).await?;
    Ok(list)
}
