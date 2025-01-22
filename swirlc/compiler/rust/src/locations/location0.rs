use std::{collections::HashMap, sync::Arc};
use tokio::task::JoinSet;
use crate::{orchestra::LocationInfo, swirl::{StepOutput, Swirl}};

pub async fn location0(location: String, address_map: HashMap<String, LocationInfo>) {
  println!("Running {}", location);

  let swirl = Arc::new(Swirl::new(location.clone(), address_map, "./workdir/location0".into()));

  let join_set = JoinSet::new();

  swirl.exec(
    "s1".to_string(), // name
    "step s1".to_string(), // display name
    vec![],
    Some("p1".into()), // output port
    StepOutput::File("message.txt".to_string()), // output type
    "ls".to_string(), // command
    vec![ // arguments
      "> message.txt".into(),
    ]
  ).await;

  // let join_set = swirl.send("p1".into(), "location1".into(), join_set).await;

  let join_set = swirl.broadcast(
    "p1".into(),
    vec!["location1".into(), "location2".into()],
    join_set
  ).await;

  join_set.join_all().await;

  println!("{} finished", location);
}