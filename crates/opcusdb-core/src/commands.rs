//! Deferred structural changes (the command buffer).
//!
//! See `CORE_SPEC.md` §8. Systems must not spawn/despawn/insert/remove while they
//! iterate, that would invalidate the storage they are walking. Instead they
//! record intentions into a [`Commands`] buffer, which the caller (later: the
//! scheduler) [`apply`](Commands::apply)s at a deterministic barrier after the
//! system runs.
//!
//! Determinism: commands are kept in a single `Vec` and applied in push order;
//! deferred spawns allocate ids via the world's deterministic allocator at apply
//! time. So the same sequence of recorded commands always produces the same world.

use crate::entity::EntityId;
use crate::world::World;

/// A deferred mutation of the world.
type WorldOp = Box<dyn FnOnce(&mut World)>;
/// A deferred component insertion onto a freshly-spawned entity.
type SpawnInserter = Box<dyn FnOnce(&mut World, EntityId)>;

/// One queued structural change.
enum Command {
    /// Spawn a fresh entity, then run each component inserter against its new id.
    Spawn(Vec<SpawnInserter>),
    /// Any other deferred mutation (despawn / insert-on-existing / remove).
    Apply(WorldOp),
}

/// A buffer of structural changes to apply later, in order.
#[derive(Default)]
pub struct Commands {
    queue: Vec<Command>,
}

impl Commands {
    /// A fresh, empty command buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a deferred spawn. Attach components with [`EntityCommands::insert`];
    /// the spawn is enqueued when the returned builder is dropped.
    ///
    /// ```ignore
    /// cmd.spawn().insert(Position(0)).insert(Velocity(1));
    /// ```
    pub fn spawn(&mut self) -> EntityCommands<'_> {
        EntityCommands {
            cmds: self,
            inserters: Vec::new(),
        }
    }

    /// Queue despawning `id` (and all its components) at apply time.
    pub fn despawn(&mut self, id: EntityId) {
        self.queue.push(Command::Apply(Box::new(move |w| {
            w.despawn(id);
        })));
    }

    /// Queue inserting `value` onto an already-live `id`. No-op at apply time if
    /// the entity is no longer alive (e.g. it was despawned earlier in the batch).
    pub fn insert<T: Clone + 'static>(&mut self, id: EntityId, value: T) {
        self.queue.push(Command::Apply(Box::new(move |w| {
            if w.is_alive(id) {
                w.insert(id, value);
            }
        })));
    }

    /// Queue removing component `T` from `id` at apply time.
    pub fn remove<T: Clone + 'static>(&mut self, id: EntityId) {
        self.queue.push(Command::Apply(Box::new(move |w| {
            w.remove::<T>(id);
        })));
    }

    /// Number of queued commands (a deferred spawn counts as one).
    #[inline]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether nothing is queued.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Apply every queued command to `world`, in push order, then leave the buffer
    /// empty for reuse next tick.
    pub fn apply(&mut self, world: &mut World) {
        for cmd in self.queue.drain(..) {
            match cmd {
                Command::Spawn(inserters) => {
                    let id = world.spawn();
                    for ins in inserters {
                        ins(world, id);
                    }
                }
                Command::Apply(f) => f(world),
            }
        }
    }
}

/// Builder for a single deferred spawn. Enqueues the spawn (with its accumulated
/// component inserters) when dropped.
pub struct EntityCommands<'a> {
    cmds: &'a mut Commands,
    inserters: Vec<SpawnInserter>,
}

impl EntityCommands<'_> {
    /// Attach `value` to the entity that will be spawned. Chainable. The spawn is
    /// enqueued on drop, so discarding the return value (ending the chain) is the
    /// intended way to finish, hence no `#[must_use]`.
    pub fn insert<T: Clone + 'static>(mut self, value: T) -> Self {
        self.inserters.push(Box::new(move |w, id| {
            w.insert(id, value);
        }));
        self
    }
}

impl Drop for EntityCommands<'_> {
    fn drop(&mut self) {
        let inserters = core::mem::take(&mut self.inserters);
        self.cmds.queue.push(Command::Spawn(inserters));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Pos(i32);
    #[derive(Clone, Debug, PartialEq)]
    struct Vel(i32);

    #[test]
    fn deferred_spawn_applies_with_components() {
        let mut w = World::new();
        let mut cmd = Commands::new();
        cmd.spawn().insert(Pos(3)).insert(Vel(4));
        assert_eq!(w.entity_count(), 0, "nothing happens before apply");
        assert_eq!(cmd.len(), 1);

        cmd.apply(&mut w);
        assert_eq!(w.entity_count(), 1);
        assert!(cmd.is_empty(), "buffer drained after apply");

        // The spawned entity is the only one; find it via a query.
        let found: Vec<_> = w.query2::<Pos, Vel>().map(|(_, p, v)| (p.0, v.0)).collect();
        assert_eq!(found, vec![(3, 4)]);
    }

    #[test]
    fn deferred_despawn_insert_remove() {
        let mut w = World::new();
        let e = w.spawn();
        w.insert(e, Pos(1));

        let mut cmd = Commands::new();
        cmd.insert(e, Vel(9));
        cmd.remove::<Pos>(e);
        cmd.apply(&mut w);
        assert_eq!(w.get::<Vel>(e), Some(&Vel(9)));
        assert!(!w.has::<Pos>(e));

        let mut cmd2 = Commands::new();
        cmd2.despawn(e);
        cmd2.apply(&mut w);
        assert!(!w.is_alive(e));
    }

    #[test]
    fn commands_apply_in_push_order() {
        // Spawn an entity, then despawn it, in the same batch: ordering must hold,
        // so the world ends empty.
        let mut w = World::new();
        let pre = w.spawn(); // an existing entity to despawn deterministically
        w.insert(pre, Pos(0));

        let mut cmd = Commands::new();
        cmd.despawn(pre); // op 1
        cmd.spawn().insert(Pos(7)); // op 2: reuses pre's freed slot
        cmd.apply(&mut w);

        assert!(!w.is_alive(pre));
        let vals: Vec<_> = w.query::<Pos>().map(|(_, p)| p.0).collect();
        assert_eq!(vals, vec![7], "exactly the spawned entity remains");
        assert_eq!(w.entity_count(), 1);
    }

    #[test]
    fn insert_on_despawned_is_noop() {
        let mut w = World::new();
        let e = w.spawn();
        let mut cmd = Commands::new();
        cmd.despawn(e); // first
        cmd.insert(e, Pos(5)); // then insert on the now-dead id -> ignored
        cmd.apply(&mut w);
        assert!(!w.is_alive(e));
        assert_eq!(w.get::<Pos>(e), None);
    }

    #[test]
    fn empty_apply_is_fine() {
        let mut w = World::new();
        let mut cmd = Commands::new();
        cmd.apply(&mut w);
        assert_eq!(w.entity_count(), 0);
    }
}
