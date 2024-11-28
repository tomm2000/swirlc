use crate::config::LocationID;

pub fn data_size(size: usize) -> String {
  let kb = 1024;
  let mb = kb * 1024;
  let gb = mb * 1024;
  let tb = gb * 1024;

  if size < kb {
    format!("{} B", size)
  } else if size < mb {
    format!("{:.2} KB", size as f64 / kb as f64)
  } else if size < gb {
    format!("{:.2} MB", size as f64 / mb as f64)
  } else if size < tb {
    format!("{:.2} GB", size as f64 / gb as f64)
  } else {
    format!("{:.2} TB", size as f64 / tb as f64)
  }
}

pub fn debug_prelude(location: &LocationID, steap_name: Option<&String>) -> String {
  // [HH:MM:SS] [location] [step_name] >>>
  let time = chrono::Local::now().format("%H:%M:%S").to_string();
  let location = format!("{:?}", location);
  let step_name = match steap_name {
    Some(step_name) => format!(" [{}]", step_name),
    None => "".to_string(),
  };

  format!("[{}] [{}]{} >>> ", time, location, step_name)
}