//! Типизированные события: очереди, Reader/Writer, отложенные на кадр.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

struct Queue<E> {
    current: Mutex<Vec<E>>,
    pending: Mutex<Vec<E>>,
}

impl<E> Queue<E> {
    fn new() -> Self {
        Self {
            current: Mutex::new(Vec::new()),
            pending: Mutex::new(Vec::new()),
        }
    }

    fn send(&self, event: E) {
        self.pending.lock().push(event);
    }

    fn update(&self) {
        let mut pending = self.pending.lock();
        let mut current = self.current.lock();
        current.clear();
        current.append(&mut pending);
    }

    fn read_from(&self, cursor: &mut usize) -> Vec<E>
    where
        E: Clone,
    {
        let current = self.current.lock();
        if *cursor > current.len() {
            *cursor = 0;
        }
        let events = current[*cursor..].to_vec();
        *cursor = current.len();
        events
    }

    fn clear(&self) {
        self.current.lock().clear();
        self.pending.lock().clear();
    }

    fn len(&self) -> usize {
        self.current.lock().len()
    }
}

trait QueueDyn: Send + Sync {
    fn update(&self);
    fn clear(&self);
}

impl<E: Send + Sync + 'static> QueueDyn for Queue<E> {
    fn update(&self) {
        Queue::update(self);
    }

    fn clear(&self) {
        Queue::clear(self);
    }
}

struct Entry {
    typed: Arc<dyn QueueDyn>,
    any: Arc<dyn Any + Send + Sync>,
}

/// Реестр типизированных очередей событий.
#[derive(Default)]
pub struct EventRegistry {
    queues: RwLock<HashMap<TypeId, Entry>>,
}

impl EventRegistry {
    /// Создаёт пустой реестр.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn queue<E>(&self) -> Arc<Queue<E>>
    where
        E: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<E>();
        let mut map = self.queues.write();

        if let Some(entry) = map.get(&type_id) {
            if let Ok(queue) = entry.any.clone().downcast::<Queue<E>>() {
                return queue;
            }
        }

        let queue = Arc::new(Queue::<E>::new());
        map.insert(
            type_id,
            Entry {
                typed: Arc::clone(&queue) as Arc<dyn QueueDyn>,
                any: Arc::clone(&queue) as Arc<dyn Any + Send + Sync>,
            },
        );
        queue
    }

    /// Writer для типа события.
    #[must_use]
    pub fn writer<E>(&self) -> Writer<E>
    where
        E: Send + Sync + 'static,
    {
        Writer {
            queue: self.queue::<E>(),
        }
    }

    /// Reader для типа события.
    #[must_use]
    pub fn reader<E>(&self) -> Reader<E>
    where
        E: Send + Sync + 'static,
    {
        Reader {
            queue: self.queue::<E>(),
            cursor: 0,
        }
    }

    /// Публикует накопленные события (раз в кадр).
    pub fn update(&self) {
        for entry in self.queues.read().values() {
            entry.typed.update();
        }
    }

    /// Очищает все очереди.
    pub fn clear_all(&self) {
        for entry in self.queues.read().values() {
            entry.typed.clear();
        }
    }

    /// Число опубликованных событий типа.
    #[must_use]
    pub fn len<E>(&self) -> usize
    where
        E: Send + Sync + 'static,
    {
        self.queues
            .read()
            .get(&TypeId::of::<E>())
            .and_then(|entry| entry.any.clone().downcast::<Queue<E>>().ok())
            .map_or(0, |queue| queue.len())
    }

    /// Нет ли опубликованных событий типа.
    #[must_use]
    pub fn is_empty<E>(&self) -> bool
    where
        E: Send + Sync + 'static,
    {
        self.len::<E>() == 0
    }
}

/// Отправитель событий.
pub struct Writer<E> {
    queue: Arc<Queue<E>>,
}

impl<E> Writer<E> {
    /// Отправляет событие (будет опубликовано на `update`).
    pub fn send(&self, event: E) {
        self.queue.send(event);
    }
}

/// Читатель событий с курсором.
pub struct Reader<E> {
    queue: Arc<Queue<E>>,
    cursor: usize,
}

impl<E: Clone> Reader<E> {
    /// Читает новые события с прошлого вызова.
    pub fn read(&mut self) -> Vec<E> {
        self.queue.read_from(&mut self.cursor)
    }

    /// Сбрасывает курсор.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Hit(u32);

    #[derive(Clone, Debug, PartialEq)]
    struct Spawn(&'static str);

    #[test]
    fn events_are_published_on_update() {
        let registry = EventRegistry::new();
        let writer = registry.writer::<Hit>();
        let mut reader = registry.reader::<Hit>();

        writer.send(Hit(1));
        assert!(reader.read().is_empty());

        registry.update();
        assert_eq!(reader.read(), vec![Hit(1)]);
        assert!(reader.read().is_empty());
    }

    #[test]
    fn types_are_isolated() {
        let registry = EventRegistry::new();
        registry.writer::<Hit>().send(Hit(7));
        registry.writer::<Spawn>().send(Spawn("npc"));

        registry.update();
        assert_eq!(registry.len::<Hit>(), 1);
        assert_eq!(registry.len::<Spawn>(), 1);
        assert_eq!(registry.reader::<Hit>().read(), vec![Hit(7)]);
    }

    #[test]
    fn update_replaces_previous_frame() {
        let registry = EventRegistry::new();
        let writer = registry.writer::<Hit>();
        let mut reader = registry.reader::<Hit>();

        writer.send(Hit(1));
        registry.update();
        assert_eq!(reader.read().len(), 1);

        registry.update();
        assert!(reader.read().is_empty());
    }
}
