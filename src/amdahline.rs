use std::{io::{BufWriter, Write}, sync::{Arc, RwLock}};

pub struct Amdahline {
  writer: Arc<RwLock<BufWriter<std::fs::File>>>
}

impl Amdahline {
  pub fn new(output_file: String) -> Self {
    // create the output file
    std::fs::File::create(&output_file).unwrap();

    let file = std::fs::OpenOptions::new().write(true).open(&output_file).unwrap();

    Self {
      writer: Arc::new(RwLock::new(BufWriter::new(file)))
    }
  }

  pub fn close(&self) {
    self.writer.write().unwrap().flush().unwrap();
    self.writer.write().unwrap().get_mut().sync_all().unwrap();
    
    // close the file
    drop(self.writer.write().unwrap());
  }

  pub fn register_executor(&self, executor_id: String) {
    let write = self.writer.write();

    let time = chrono::Local::now().format("%H:%M:%S:%f").to_string();
    let message = format!("[{}] REGISTERED <{}>\n", time, executor_id);

    write.unwrap().write_all(message.as_bytes()).unwrap();
  }

  pub fn unregister_executor(&self, executor_id: String) {
    let write = self.writer.write();

    let time = chrono::Local::now().format("%H:%M:%S:%f").to_string();
    let message = format!("[{}] UNREGISTERED <{}>\n", time, executor_id);

    write.unwrap().write_all(message.as_bytes()).unwrap();
  }

  pub fn begin_task(&self, executor_id: String, task: String) -> uuid::Uuid {
    let write = self.writer.write();

    let uuid: uuid::Uuid = uuid::Uuid::new_v4();
    let time = chrono::Local::now().format("%H:%M:%S:%f").to_string();

    let message = format!("[{}] BEGIN <{}> <{}> \"{}\"\n", time, executor_id, uuid, task);

    write.unwrap().write_all(message.as_bytes()).unwrap();

    uuid
  }

  pub fn end_task(&self, executor_id: String, uuid: uuid::Uuid) {
    let write = self.writer.write();

    let time = chrono::Local::now().format("%H:%M:%S:%f").to_string();
    let message = format!("[{}] END <{}> <{}>\n", time, executor_id, uuid);

    write.unwrap().write_all(message.as_bytes()).unwrap();
  }
}