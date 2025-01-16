use std::{path::PathBuf, sync::Arc};

use bytes::Bytes;
use tokio::task::JoinSet;

use crate::orchestra::LocationID;

use super::{PortData, PortID, Swirl};

impl Swirl {
  pub async fn broadcast(
    self: &Arc<Self>,
    port_id: PortID,
    destination: Vec<String>,
    join_set: JoinSet<()>,
  ) -> JoinSet<()> {





    return join_set;
  }
}