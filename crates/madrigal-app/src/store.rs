use madrigal_player::queue::Queue;
use madrigal_settings::Settings;
use rusqlite::Connection;

pub struct Store {
    settings: Settings,
    db: Connection,
    queue: Queue,
}

impl Store {
    pub fn open() -> Result<Self, String> {
        madrigal_paths::ensure_dirs().map_err(|err| format!("ensure_dirs: {err}"))?;
        let settings = Settings::load();
        let db = madrigal_db::open_default().map_err(|err| format!("db: {err}"))?;
        let queue = Queue::new();
        Ok(Self {
            settings,
            db,
            queue,
        })
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    pub fn db(&self) -> &Connection {
        &self.db
    }

    pub fn db_mut(&mut self) -> &mut Connection {
        &mut self.db
    }

    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    pub fn queue_mut(&mut self) -> &mut Queue {
        &mut self.queue
    }
}
