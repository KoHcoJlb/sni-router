use std::{collections::HashMap, sync::Arc};

use parking_lot::RwLock;

use crate::{config::Config, router::Route};

pub struct Server {
    pub config: Config,
    pub routes: RwLock<Arc<HashMap<String, Route>>>,
}
