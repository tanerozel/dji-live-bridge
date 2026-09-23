use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Shared publish-route ownership table used by the built-in server to enforce
/// single-publisher-per-route without storing non-UnwindSafe closure trait objects
/// on `Conn`.
#[derive(Clone)]
pub(crate) struct PublishRouteRegistry {
    routes: Arc<Mutex<HashMap<(String, String), u64>>>,
}

impl PublishRouteRegistry {
    pub(crate) fn new(routes: Arc<Mutex<HashMap<(String, String), u64>>>) -> Self {
        Self { routes }
    }

    pub(crate) fn claim(&self, conn_id: u64, app: &str, stream: &str) -> bool {
        let key = (app.to_string(), stream.to_string());
        let Ok(mut map) = self.routes.lock() else {
            return false;
        };
        match map.get(&key) {
            Some(&owner) if owner != conn_id => false,
            _ => {
                map.insert(key, conn_id);
                true
            }
        }
    }

    pub(crate) fn release(&self, conn_id: u64, app: &str, stream: &str) {
        let key = (app.to_string(), stream.to_string());
        if let Ok(mut map) = self.routes.lock() {
            if map.get(&key) == Some(&conn_id) {
                map.remove(&key);
            }
        }
    }

    /// Release a route regardless of which connection currently owns it.
    ///
    /// Mirrors FMS `releaseStream` semantics: an encoder that reconnected
    /// after a network drop can force-clear a stale claim held by its own
    /// previous (now-dead) TCP connection so the immediately following
    /// `publish` does not fail with "stream already published".
    pub(crate) fn force_release(&self, app: &str, stream: &str) {
        let key = (app.to_string(), stream.to_string());
        if let Ok(mut map) = self.routes.lock() {
            map.remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> PublishRouteRegistry {
        PublishRouteRegistry::new(Arc::new(Mutex::new(HashMap::new())))
    }

    #[test]
    fn force_release_clears_a_route_owned_by_another_connection() {
        let reg = registry();
        assert!(reg.claim(1, "live", "cam1"));
        assert!(!reg.claim(2, "live", "cam1"));

        reg.force_release("live", "cam1");

        assert!(reg.claim(2, "live", "cam1"));
    }

    #[test]
    fn force_release_on_an_unclaimed_route_is_a_no_op() {
        let reg = registry();
        reg.force_release("live", "cam1");
        assert!(reg.claim(1, "live", "cam1"));
    }
}
