use crate::{Loader, Method, Plugin, Request, RequestHandler, StatusCode};
use path_tree::PathTree;
use serde_json as json;
use std::collections::HashMap;
use std::fmt;
use std::iter::Iterator;
use std::sync::{Arc, Mutex};

type PluginHandler = (Plugin, Arc<dyn RequestHandler>);

/// Plugin to keep track of registered plugins
pub(crate) struct PluginRegistry {
    plugins: Mutex<HashMap<String, PluginHandler>>,
    routes: Mutex<PathTree<String>>,
}

impl PluginRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(PluginRegistry {
            plugins: Mutex::new(HashMap::new()),
            routes: Mutex::new(PathTree::new()),
        })
    }

    pub fn match_plugin_handler(&self, path: &str) -> Option<PluginHandler> {
        let routes = self.routes.lock().unwrap();
        let plugins = self.plugins.lock().unwrap();
        let (name, _) = routes.find(path)?;
        let (plugin, handler) = plugins.get(name)?;
        Some((plugin.clone(), handler.clone()))
    }

    pub fn register(&self, plugin: Plugin, handler: Box<dyn RequestHandler>) {
        let mut routes = self.routes.lock().unwrap();
        let mut plugins = self.plugins.lock().unwrap();
        routes.insert(&plugin.prefix(), plugin.name().into());
        plugins.insert(plugin.name().into(), (plugin, handler.into()));
    }

    fn plugin_list(&self) -> Vec<Plugin> {
        self.plugins
            .lock()
            .unwrap()
            .values()
            .map(|(p, _)| p.clone())
            .collect()
    }

    pub fn as_handler(self: Arc<Self>, loader: Arc<impl Loader>) -> Box<dyn RequestHandler> {
        Box::new(move |mut req: Request| {
            let registry = self.clone();
            let loader = loader.clone();
            async move {
                match req.method() {
                    Method::Get => {
                        let plugins = registry.plugin_list();
                        json::to_vec(&plugins).map_or(
                            res!(StatusCode::InternalServerError),
                            |list| {
                                res!(list, {
                                    content_type: "application/json",
                                })
                            },
                        )
                    }
                    Method::Post => match req.body_json().await {
                        Ok(plugin) => match loader.load(&plugin).await {
                            Ok(handler) => {
                                registry.register(plugin, handler);
                                res!(StatusCode::Created)
                            }
                            Err(_) => {
                                res!(StatusCode::UnprocessableEntity, "Can't load plugin")
                            }
                        },
                        Err(e) => res!(StatusCode::BadRequest, e.to_string()),
                    },
                    _ => res!(StatusCode::MethodNotAllowed),
                }
            }
        })
    }
}

impl fmt::Debug for PluginRegistry
where
    for<'a> dyn RequestHandler + 'a: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PluginRegistry")
            .field("plugins", &self.plugins)
            .field("routes", &self.routes)
            .finish()
    }
}
