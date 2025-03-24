use std::{collections::HashMap, sync::Arc};
use crate::{orchestra::{broadcast::destinations_ntree_advanced, LocationInfo}, swirl::Swirl};

pub async fn location0(location: String, address_map: HashMap<String, LocationInfo>) {
  println!("Running {}", location);

  let swirl = Arc::new(Swirl::new(location.clone(), address_map, "./workdir/location0".into()));

  let destinations: Vec<String> = swirl.orchestra.locations().iter().map(|location| location.0.clone()).collect();

  println!("Destinations: {:?}", destinations);

  let destinations: Vec<u16> = destinations.iter().map(|location| swirl.orchestra.location_id(location)).collect();
  // remove the current location from the destinations
  let destinations = destinations.iter().filter(|&&location| location != swirl.orchestra.location).copied().collect();

  let tree = destinations_ntree_advanced(swirl.orchestra.location, destinations, &swirl.orchestra);

  println!("{}", tree.display(&swirl.orchestra));
}