//! Profiles of other users and the follow relations the host keeps.

use super::Events;
use crate::data::{MaybeSend, Reply};

/// The host's identifier of a profile.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileId(
    /// The host's own identifier, unique among its profiles.
    pub String,
);

/// What the host knows about a user the signed-in user can follow.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Profile {
    /// The host's identifier.
    pub id: ProfileId,
    /// Display name.
    pub name: String,
    /// Short description the user wrote. May be empty.
    pub bio: String,
    /// URI of the user's picture, for egui's installed image loaders.
    pub avatar: Option<String>,
    /// How many users follow this one, when the host reports it.
    pub followers: Option<u64>,
    /// Whether the signed-in user follows this user, as the host last reported.
    pub followed: bool,
}

impl Profile {
    /// A profile that is not followed, without a bio, picture or follower count.
    pub fn new(id: ProfileId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            bio: String::new(),
            avatar: None,
            followers: None,
            followed: false,
        }
    }
}

/// The host's profiles and follow relations. The host keeps the account, decides who may follow
/// whom and what a profile shows; the library shows what the host pushes and sends the user's
/// follow and unfollow requests through [`follow`](Self::follow). It never changes
/// [`Profile::followed`] itself.
///
/// Methods are called on the UI thread and must return at once; answer and push from any
/// thread, runtime or browser task.
///
/// ```
/// use std::sync::Mutex;
///
/// use egui_charts::data::Reply;
/// use egui_charts::services::{Events, Profile, ProfileEvent, ProfileId, Profiles};
///
/// /// One profile that accepts every follow request. A real host asks its server.
/// #[derive(Default)]
/// struct Local(Mutex<Option<Events<ProfileEvent>>>);
///
/// impl Profiles for Local {
///     fn watch(&self, events: Events<ProfileEvent>) {
///         events.push(ProfileEvent::Upsert(Profile::new(ProfileId("1".into()), "Ada")));
///         *self.0.lock().unwrap() = Some(events);
///     }
///
///     fn follow(&self, id: ProfileId, follow: bool, reply: Reply<()>) {
///         if let Some(events) = &*self.0.lock().unwrap() {
///             let mut profile = Profile::new(id, "Ada");
///             profile.followed = follow;
///             events.push(ProfileEvent::Upsert(profile));
///         }
///         reply.send(Ok(()));
///     }
/// }
/// ```
pub trait Profiles: MaybeSend + 'static {
    /// Pushes the profiles the user can see, then every change until `events` is inactive. Called
    /// when the directory starts and again after a lagged stream, each time with a new stream that
    /// starts from no profiles. The stream takes at most 10,000 pushes between two frames.
    fn watch(&self, events: Events<ProfileEvent>);

    /// Follows `id` when `follow` is true, otherwise unfollows it. Push the profile with its new
    /// [`Profile::followed`] as [`ProfileEvent::Upsert`] first, then answer `Ok` once the relation
    /// changed, or with a [`FeedError`](crate::data::FeedError) the user can read if the host refused. Authorisation
    /// belongs to the host: refuse a request the signed-in user may not make.
    fn follow(&self, id: ProfileId, follow: bool, reply: Reply<()>);
}

/// A change to the host's profiles, pushed through the stream [`Profiles::watch`] received.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ProfileEvent {
    /// Adds the profile, or replaces the one with its id.
    Upsert(Profile),
    /// Removes the profile with this id, if any.
    Remove(ProfileId),
    /// Removes every profile, so a host that reconnects can push them again.
    Clear,
}
