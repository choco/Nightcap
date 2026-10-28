//! The grants and the login item as macOS reports them, beside the choices their checks compare
//! them with.

use nightcap_protocol::{CheckKind, LoginItem, NotificationPermission};

/// The grants and the login item as macOS last reported them, and sunset to sunrise set aside,
/// each with the choice its check compares it with; and the problems already announced.
pub(crate) struct Grants {
    /// None until the shell first reports it.
    pub(crate) accessibility: Option<bool>,
    pub(crate) notifications: NotificationPermission,
    /// See `saved_state::Persisted::notifications_wanted`.
    pub(crate) notifications_wanted: bool,
    /// None until the shell first reports it.
    pub(crate) login_item: Option<LoginItem>,
    /// See `saved_state::Persisted::open_at_login`.
    pub(crate) open_at_login: bool,
    /// macOS has sunset to sunrise set aside, and the schedule is off.
    pub(crate) schedule_set_aside: bool,
    /// See `saved_state::Persisted::schedule_off_accepted`.
    pub(crate) schedule_off_accepted: bool,
    /// See `saved_state::Persisted::announced`.
    pub(crate) announced: Vec<CheckKind>,
}

impl Grants {
    pub(crate) fn login_item_changed(&mut self, item: LoginItem) {
        tracing::info!("login item: {item:?}");
        self.login_item = Some(item);
        // An enabled item becomes the user's wish, however it got enabled.
        if item == LoginItem::Enabled {
            self.open_at_login = true;
        }
    }

    pub(crate) fn permissions_changed(
        &mut self,
        accessibility: bool,
        notifications: NotificationPermission,
    ) {
        tracing::info!(
            "permissions: accessibility {accessibility}, notifications {notifications:?}"
        );
        self.accessibility = Some(accessibility);
        self.notifications = notifications;
        // Allowed notifications become the user's wish, however they got allowed.
        if notifications == NotificationPermission::Granted {
            self.notifications_wanted = true;
        }
    }

    /// Whether macOS has sunset to sunrise set aside, as a status callback reports it with the
    /// schedule off. Accepting it ends once it isn't.
    pub(crate) fn schedule_reported(&mut self, set_aside: bool) {
        self.schedule_set_aside = set_aside;
        if !self.schedule_set_aside {
            self.schedule_off_accepted = false;
        }
    }
}
