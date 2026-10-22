//! A bounded FIFO response cache so identical requests skip the provider.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use crate::types::{ChatRequest, ChatResponse};

/// A bounded response cache keyed by a hash of the request and task.
#[derive(Debug)]
pub struct ResponseCache {
    capacity: usize,
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    map: HashMap<u64, ChatResponse>,
    order: VecDeque<u64>,
    hits: u64,
}

impl ResponseCache {
    /// Create a cache holding at most `capacity` responses.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// The cache key for a request under `task`.
    pub fn key(request: &ChatRequest, task: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        task.hash(&mut hasher);
        // `ChatRequest` serializes deterministically enough for caching.
        if let Ok(json) = serde_json::to_string(request) {
            json.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Look up a cached response, counting a hit.
    pub fn get(&self, key: u64) -> Option<ChatResponse> {
        let mut inner = self.inner.lock().expect("response cache poisoned");
        if let Some(response) = inner.map.get(&key).cloned() {
            inner.hits += 1;
            return Some(response);
        }
        None
    }

    /// Insert a response, evicting the oldest entries beyond capacity.
    pub fn put(&self, key: u64, response: ChatResponse) {
        let mut inner = self.inner.lock().expect("response cache poisoned");
        if inner.map.insert(key, response).is_none() {
            inner.order.push_back(key);
        }
        while inner.order.len() > self.capacity {
            if let Some(oldest) = inner.order.pop_front() {
                inner.map.remove(&oldest);
            }
        }
    }

    /// The number of cache hits.
    pub fn hits(&self) -> u64 {
        self.inner.lock().expect("response cache poisoned").hits
    }

    /// The number of cached responses.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("response cache poisoned")
            .map
            .len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatResponse;

    fn response(text: &str) -> ChatResponse {
        crate::provider::response_from_text("m", text, crate::types::Usage::default())
    }

    #[test]
    fn evicts_oldest_beyond_capacity() {
        let cache = ResponseCache::new(2);
        cache.put(1, response("a"));
        cache.put(2, response("b"));
        cache.put(3, response("c"));
        assert_eq!(cache.len(), 2);
        assert!(cache.get(1).is_none());
        assert!(cache.get(3).is_some());
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn keys_differ_by_task() {
        let request = ChatRequest::user("m", "hi");
        assert_ne!(
            ResponseCache::key(&request, "chat"),
            ResponseCache::key(&request, "summarize")
        );
    }
}
