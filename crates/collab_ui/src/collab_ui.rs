pub mod channel_view;
pub mod chat_panel;
pub mod collab_panel;
mod collab_titlebar_item;
mod contact_notification;
mod face_pile;
mod incoming_call_notification;
pub mod notification_panel;
mod notifications;
mod panel_settings;
pub mod project_shared_notification;
mod sharing_status_indicator;

use call::{report_call_event_for_room, ActiveCall, Room};
use feature_flags::{ChannelsAlpha, FeatureFlagAppExt};
use gpui::{
    actions,
    elements::{Empty, Image},
    geometry::{
        rect::RectF,
        vector::{vec2f, Vector2F},
    },
    platform::{Screen, WindowBounds, WindowKind, WindowOptions},
    AnyElement, AppContext, Element, ImageData, Task,
};
use std::{rc::Rc, sync::Arc};
use theme::Theme;
use time::{OffsetDateTime, UtcOffset};
use util::ResultExt;
use workspace::AppState;

pub use collab_titlebar_item::CollabTitlebarItem;
pub use panel_settings::{
    ChatPanelSettings, CollaborationPanelSettings, NotificationPanelSettings,
};

actions!(
    collab,
    [ToggleScreenSharing, ToggleMute, ToggleDeafen, LeaveCall]
);

pub fn init(app_state: &Arc<AppState>, cx: &mut AppContext) {
    settings::register::<CollaborationPanelSettings>(cx);
    settings::register::<ChatPanelSettings>(cx);
    settings::register::<NotificationPanelSettings>(cx);

    vcs_menu::init(cx);
    collab_titlebar_item::init(cx);
    collab_panel::init(cx);
    chat_panel::init(cx);
    incoming_call_notification::init(&app_state, cx);
    project_shared_notification::init(&app_state, cx);
    sharing_status_indicator::init(cx);

    cx.add_global_action(toggle_screen_sharing);
    cx.add_global_action(toggle_mute);
    cx.add_global_action(toggle_deafen);
}

pub fn toggle_screen_sharing(_: &ToggleScreenSharing, cx: &mut AppContext) {
    let call = ActiveCall::global(cx).read(cx);
    if let Some(room) = call.room().cloned() {
        let client = call.client();
        let toggle_screen_sharing = room.update(cx, |room, cx| {
            if room.is_screen_sharing() {
                report_call_event_for_room(
                    "disable screen share",
                    room.id(),
                    room.channel_id(),
                    &client,
                    cx,
                );
                Task::ready(room.unshare_screen(cx))
            } else {
                report_call_event_for_room(
                    "enable screen share",
                    room.id(),
                    room.channel_id(),
                    &client,
                    cx,
                );
                room.share_screen(cx)
            }
        });
        toggle_screen_sharing.detach_and_log_err(cx);
    }
}

pub fn toggle_mute(_: &ToggleMute, cx: &mut AppContext) {
    let call = ActiveCall::global(cx).read(cx);
    if let Some(room) = call.room().cloned() {
        let client = call.client();
        room.update(cx, |room, cx| {
            let operation = if room.is_muted(cx) {
                "enable microphone"
            } else {
                "disable microphone"
            };
            report_call_event_for_room(operation, room.id(), room.channel_id(), &client, cx);

            room.toggle_mute(cx)
        })
        .map(|task| task.detach_and_log_err(cx))
        .log_err();
    }
}

pub fn toggle_deafen(_: &ToggleDeafen, cx: &mut AppContext) {
    if let Some(room) = ActiveCall::global(cx).read(cx).room().cloned() {
        room.update(cx, Room::toggle_deafen)
            .map(|task| task.detach_and_log_err(cx))
            .log_err();
    }
}

fn notification_window_options(
    screen: Rc<dyn Screen>,
    window_size: Vector2F,
) -> WindowOptions<'static> {
    const NOTIFICATION_PADDING: f32 = 16.;

    let screen_bounds = screen.content_bounds();
    WindowOptions {
        bounds: WindowBounds::Fixed(RectF::new(
            screen_bounds.upper_right()
                + vec2f(
                    -NOTIFICATION_PADDING - window_size.x(),
                    NOTIFICATION_PADDING,
                ),
            window_size,
        )),
        titlebar: None,
        center: false,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        screen: Some(screen),
    }
}

fn render_avatar<T: 'static>(avatar: Option<Arc<ImageData>>, theme: &Arc<Theme>) -> AnyElement<T> {
    let avatar_style = theme.chat_panel.avatar;
    avatar
        .map(|avatar| {
            Image::from_data(avatar)
                .with_style(avatar_style.image)
                .aligned()
                .contained()
                .with_corner_radius(avatar_style.outer_corner_radius)
                .constrained()
                .with_width(avatar_style.outer_width)
                .with_height(avatar_style.outer_width)
                .into_any()
        })
        .unwrap_or_else(|| {
            Empty::new()
                .constrained()
                .with_width(avatar_style.outer_width)
                .into_any()
        })
        .contained()
        .with_style(theme.chat_panel.avatar_container)
        .into_any()
}

fn format_timestamp(
    mut timestamp: OffsetDateTime,
    mut now: OffsetDateTime,
    local_timezone: UtcOffset,
) -> String {
    timestamp = timestamp.to_offset(local_timezone);
    now = now.to_offset(local_timezone);

    let today = now.date();
    let date = timestamp.date();
    let mut hour = timestamp.hour();
    let mut part = "am";
    if hour > 12 {
        hour -= 12;
        part = "pm";
    }
    if date == today {
        format!("{:02}:{:02}{}", hour, timestamp.minute(), part)
    } else if date.next_day() == Some(today) {
        format!("yesterday at {:02}:{:02}{}", hour, timestamp.minute(), part)
    } else {
        format!("{:02}/{}/{}", date.month() as u32, date.day(), date.year())
    }
}

fn is_channels_feature_enabled(cx: &gpui::WindowContext<'_>) -> bool {
    cx.is_staff() || cx.has_flag::<ChannelsAlpha>()
}
