//! The profiles and follow requests of one account, for the terminal's social panels.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use super::{Listener, Profile, ProfileEvent, ProfileId, Profiles};
use crate::data::FeedError;
use crate::data::inbox::Inbox;

/// Where a follow or unfollow request stands.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FollowStatus {
    /// Sent to the host, which has not answered.
    Pending,
    /// The host refused or could not make the change; the message is the host's. The profile
    /// keeps its last reported state.
    Failed(String),
}

/// A request to follow or unfollow, and how it stands.
#[derive(Clone, Debug, PartialEq)]
pub struct FollowOperation {
    /// True for a follow request, false for an unfollow request.
    pub follow: bool,
    /// Where it stands.
    pub status: FollowStatus,
}

/// The profiles the host pushed and the follow requests the user made, for one account. Call
/// [`poll`](Self::poll) every frame; show [`profile`](Self::profile) with its
/// [`operation`](Self::operation), which says whether a request is pending or failed.
///
/// A new service starts from nothing: [`set_service`](Self::set_service) drops the previous
/// account's profiles and requests, and answers still on their way from the old service are
/// ignored.
#[derive(Default)]
pub struct ProfileDirectory {
    service: Option<Arc<dyn Profiles>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    generation: u64,
    profiles: BTreeMap<ProfileId, Profile>,
    operations: HashMap<ProfileId, FollowOperation>,
    /// The key of each reply awaited, with the profile it concerns.
    requests: HashMap<u64, ProfileId>,
    /// Requests sent to the current service, which numbers their keys.
    sent: u64,
}

impl ProfileDirectory {
    /// A directory without a service. Attach one with [`set_service`](Self::set_service).
    pub fn new() -> Self {
        Self::default()
    }

    /// Connects to `service`, dropping the previous account's profiles and follow requests.
    pub fn set_service(&mut self, service: Arc<dyn Profiles>) {
        self.service = Some(service);
        self.watch();
    }

    fn watch(&mut self) {
        self.profiles.clear();
        self.operations.clear();
        self.requests.clear();
        self.sent = 0;
        self.inbox = Arc::default();
        // Stream keys are even and request keys odd, so they never meet.
        self.generation += 2;
        let (listener, events) = Listener::new(&self.inbox, self.generation);
        self.listener = Some(listener);
        if let Some(service) = &self.service {
            service.watch(events);
        }
    }

    /// Applies the host's pushes and answers, and watches again after a lagged stream.
    pub fn poll(&mut self, ctx: &egui::Context) {
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.watch();
        }
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation {
                match answer.downcast::<ProfileEvent>().map(|event| *event) {
                    Ok(ProfileEvent::Upsert(profile)) => {
                        self.profiles.insert(profile.id.clone(), profile);
                    }
                    Ok(ProfileEvent::Remove(id)) => {
                        self.profiles.remove(&id);
                        self.operations.remove(&id);
                    }
                    Ok(ProfileEvent::Clear) => self.profiles.clear(),
                    Err(_) => {}
                }
            } else if let Some(id) = self.requests.remove(&key)
                && let Ok(answer) = answer.downcast::<Result<(), FeedError>>()
            {
                match *answer {
                    Ok(()) => {
                        self.operations.remove(&id);
                    }
                    Err(error) => {
                        if let Some(operation) = self.operations.get_mut(&id) {
                            operation.status = FollowStatus::Failed(error.to_string());
                        }
                    }
                }
            }
        }
    }

    /// The profile `id`, as the host last pushed it.
    pub fn profile(&self, id: &ProfileId) -> Option<&Profile> {
        self.profiles.get(id)
    }

    /// Every profile, ordered by identifier.
    pub fn profiles(&self) -> impl Iterator<Item = &Profile> {
        self.profiles.values()
    }

    /// The follow request for `id` that is pending or failed, if any.
    pub fn operation(&self, id: &ProfileId) -> Option<&FollowOperation> {
        self.operations.get(id)
    }

    /// Asks the host to follow (`follow`) or unfollow `id`. Ignored for an unknown profile, one
    /// already in the asked state, or while an earlier request for it is pending; a failed
    /// request may be repeated.
    pub fn follow(&mut self, id: &ProfileId, follow: bool) {
        let pending = self
            .operations
            .get(id)
            .is_some_and(|operation| operation.status == FollowStatus::Pending);
        if pending || self.profiles.get(id).is_none_or(|p| p.followed == follow) {
            return;
        }
        let Some(service) = &self.service else {
            return;
        };
        self.sent += 1;
        let key = self.generation + 2 * self.sent - 1;
        self.requests.insert(key, id.clone());
        self.operations.insert(
            id.clone(),
            FollowOperation {
                follow,
                status: FollowStatus::Pending,
            },
        );
        service.follow(id.clone(), follow, self.inbox.reply(key));
    }

    /// Forgets a failed request, such as after the user dismissed its message.
    pub fn dismiss(&mut self, id: &ProfileId) {
        if self
            .operations
            .get(id)
            .is_some_and(|operation| operation.status != FollowStatus::Pending)
        {
            self.operations.remove(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::services::Events;
    use std::sync::Mutex;

    /// A host that pushes one profile and holds the follow answers for the test to send.
    #[derive(Default)]
    struct Host(Mutex<Held>);

    /// The stream, and the follow answers held back.
    type Held = (Option<Events<ProfileEvent>>, Vec<(bool, Reply<()>)>);

    impl Profiles for Host {
        fn watch(&self, events: Events<ProfileEvent>) {
            events.push(ProfileEvent::Upsert(Profile::new(
                ProfileId("1".into()),
                "Ada",
            )));
            self.0.lock().unwrap().0 = Some(events);
        }
        fn follow(&self, _: ProfileId, follow: bool, reply: Reply<()>) {
            self.0.lock().unwrap().1.push((follow, reply));
        }
    }

    #[test]
    fn follow_requests_show_pending_and_failed_and_a_new_service_forgets_the_old_account() {
        let ctx = egui::Context::default();
        let (old, id) = (Arc::new(Host::default()), ProfileId("1".into()));
        let mut directory = ProfileDirectory::new();
        directory.set_service(old.clone());
        directory.poll(&ctx);
        assert_eq!(directory.profile(&id).map(|p| p.followed), Some(false));
        // Unfollowing a profile that is not followed asks nothing; following is pending.
        directory.follow(&id, false);
        directory.follow(&id, true);
        directory.follow(&id, true);
        let (follow, reply) = old.0.lock().unwrap().1.pop().unwrap();
        assert!(follow && old.0.lock().unwrap().1.is_empty(), "one request");
        assert_eq!(
            directory.operation(&id).unwrap().status,
            FollowStatus::Pending
        );
        reply.send(Err(FeedError::Other("blocked".into())));
        directory.poll(&ctx);
        assert!(matches!(
            &directory.operation(&id).unwrap().status,
            FollowStatus::Failed(message) if message.contains("blocked")
        ));
        assert_eq!(directory.profile(&id).map(|p| p.followed), Some(false));
        // A failed request may be repeated; the host's push and answer settle it.
        directory.follow(&id, true);
        let (_, reply) = old.0.lock().unwrap().1.pop().unwrap();
        let mut followed = Profile::new(id.clone(), "Ada");
        followed.followed = true;
        old.0
            .lock()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .push(ProfileEvent::Upsert(followed));
        reply.send(Ok(()));
        directory.poll(&ctx);
        assert!(directory.operation(&id).is_none());
        assert_eq!(directory.profile(&id).map(|p| p.followed), Some(true));
        // Another account: nothing of the old one stays, and its late answer is ignored.
        directory.follow(&id, false);
        let (_, late) = old.0.lock().unwrap().1.pop().unwrap();
        directory.set_service(Arc::new(Host::default()));
        late.send(Ok(()));
        directory.poll(&ctx);
        assert_eq!(directory.profile(&id).map(|p| p.followed), Some(false));
        assert!(directory.operation(&id).is_none());
    }
}
