use super::*;
use crate::data::{FeedError, Reply};
use crate::services::{Events, Profile, ProfileEvent, Profiles};
use egui::Role;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Host {
    events: Mutex<Option<Events<ProfileEvent>>>,
    requests: Mutex<Vec<(ProfileId, bool, Reply<()>)>>,
}

impl Profiles for Host {
    fn watch(&self, events: Events<ProfileEvent>) {
        *self.events.lock().unwrap() = Some(events);
    }

    fn follow(&self, id: ProfileId, follow: bool, reply: Reply<()>) {
        self.requests.lock().unwrap().push((id, follow, reply));
    }
}

/// The card routes both directions through the host, keeps pending requests single, shows a
/// refusal without changing the relation, and reads only the new account after a service swap.
#[test]
fn follow_controls_wait_for_the_host_and_forget_the_previous_account() {
    for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
        let host = Arc::new(Host::default());
        let id = ProfileId("ada".into());
        let mut directory = ProfileDirectory::new();
        directory.set_service(host.clone());
        let mut profile = Profile::new(id.clone(), "Ada");
        profile.bio = "Market researcher".into();
        profile.followers = Some(42);
        host.events
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .push(ProfileEvent::Upsert(profile.clone()));
        let mut h = Harness::builder()
            .with_size(egui::vec2(320.0, 400.0))
            .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
            .build_ui_state(
                |ui, directory: &mut ProfileDirectory| {
                    directory.poll(ui.ctx());
                    ProfileCard::new(&id).show(ui, directory);
                },
                directory,
            );
        h.run_steps(3);
        h.get_by_label("Ada");
        h.get_by_label("Market researcher");
        h.get_by_label("Followers: 42");
        for follow in [true, false] {
            let label = if follow { "Follow" } else { "Unfollow" };
            h.get_by_role_and_label(Role::Button, label).click();
            h.run_steps(3);
            let (requested_id, requested_follow, reply) = {
                let mut requests = host.requests.lock().unwrap();
                assert_eq!(requests.len(), 1);
                requests.pop().unwrap()
            };
            assert_eq!((requested_id, requested_follow), (id.clone(), follow));
            let pending_label = if follow {
                "Following…"
            } else {
                "Unfollowing…"
            };
            assert!(
                h.get_by_role_and_label(Role::Button, pending_label)
                    .accesskit_node()
                    .is_disabled()
            );
            h.get_by_role_and_label(Role::Button, pending_label).click();
            h.run_steps(3);
            assert!(host.requests.lock().unwrap().is_empty());
            reply.send(Err(FeedError::Other("Permission denied".into())));
            h.run_steps(3);
            h.get_by_label("Permission denied");
            assert_eq!(h.state().profile(&id).unwrap().followed, !follow);
            h.get_by_role_and_label(Role::Button, "Dismiss").click();
            h.run_steps(3);
            assert!(h.query_by_label("Permission denied").is_none());
            h.get_by_role_and_label(Role::Button, label).click();
            h.run_steps(3);
            let (_, _, reply) = host.requests.lock().unwrap().pop().unwrap();
            profile.followed = follow;
            host.events
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .push(ProfileEvent::Upsert(profile.clone()));
            reply.send(Ok(()));
            h.run_steps(3);
            assert_eq!(h.state().profile(&id).unwrap().followed, follow);
        }
        h.get_by_role_and_label(Role::Button, "Follow").click();
        h.run_steps(3);
        let (_, _, late) = host.requests.lock().unwrap().pop().unwrap();
        h.state_mut().set_service(Arc::new(Host::default()));
        late.send(Err(FeedError::Other("Old account".into())));
        h.run_steps(3);
        h.get_by_label("Profile unavailable");
        assert!(h.query_by_label("Ada").is_none());
        assert!(h.query_by_label("Follow").is_none());
        assert!(h.query_by_label("Old account").is_none());
    }
}
