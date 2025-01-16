pub async fn execute() {
}use std::{collections::HashMap, sync::Arc};

use tokio::task::JoinSet;

use crate::{orchestra::LocationInfo, swirl::Swirl};

pub async fn location2(location: String, address_map: HashMap<String, LocationInfo>) {
  println!("Running {}", location);

  let swirl = Arc::new(Swirl::new(location.clone(), address_map, "./workdir/location2".into()));

  let join_set = JoinSet::new();

  let join_set = swirl.receive("p1".into(), "location0".into(), join_set).await;

  join_set.join_all().await;

  println!("{} finished", location);
}