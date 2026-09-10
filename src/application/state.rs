use std::collections::HashMap;

use crossterm::event::KeyEvent;
use nostr_sdk::prelude::*;
use nowhear::Track;
use tears::prelude::*;
use tokio::sync::mpsc;

use crate::{
    application::config::Config,
    application::message::AppMsg,
    domain::nostr::{nip10::ReplyTagsBuilder, nip38::MusicStatus, FeedKind, Profile},
    model::{
        editor::{Editor, Message as EditorMessage},
        nostr::{Message as NostrMessage, Nostr, NostrOutcome},
        nostr_gateway::{CommandError, NostrCommand, PublishId},
        status_bar::{Message, StatusBar},
        timeline::{
            tab::TimelineOutcome, text_note::TextNote, Message as TimelineMessage, Timeline,
        },
    },
};

pub mod user;

pub use user::UserState;

/// Tracks whether the application is still starting up.
///
/// Startup lasts from launch until the first event arrives. While starting up,
/// the application ignores timeline operations so the "loading..." status
/// message is preserved. This is a startup indicator, not a data-completeness
/// signal (it does not wait for EOSE). `Default` starts in the startup state.
#[derive(Debug, Clone, Default)]
pub struct Startup {
    completed: bool,
}

impl Startup {
    /// Whether the application is still starting up
    pub fn is_in_progress(&self) -> bool {
        !self.completed
    }

    /// Mark startup as completed (idempotent)
    pub fn mark_completed(&mut self) {
        self.completed = true;
    }
}

/// Unified application state
#[derive(Debug, Default)]
pub struct AppState<'a> {
    pub timeline: Timeline,
    pub editor: Editor<'a>,
    pub user: UserState,
    pub nostr: Nostr,
    pub config: ConfigState,
    pub status_bar: StatusBar,
    pub startup: Startup,
    /// Sender for dispatching commands to the Nostr subscription worker.
    /// Owned here (not in `model::nostr`) so the application layer performs the
    /// I/O while `model` stays side-effect free; set once the worker is ready.
    command_sender: Option<mpsc::UnboundedSender<NostrCommand>>,
    /// Publishes handed to the worker and not yet answered, by the id they were sent
    /// under.
    ///
    /// Keyed rather than ordered: an outcome that never arrives leaves one stale entry
    /// instead of shifting every later answer onto the wrong submission.
    pending_publishes: HashMap<PublishId, PendingPublish>,
    /// Source of [`PublishId`]s, monotonic for the life of the application.
    next_publish_id: u64,
    /// Whether the configured key can only read — an `npub` rather than a signing key.
    ///
    /// Known here so a publish that could never succeed is refused before it is queued,
    /// rather than failing in the worker after the caller has moved on.
    read_only: bool,
}

/// Status-bar label for the now-playing line.
///
/// Deliberately not a [`PublishKind`]: it names what is playing, not what a relay took.
/// Borrowing the past-tense vocabulary would make an unconfirmed line read exactly like a
/// confirmed one, which is the ambiguity the rest of this removes. #521 covers whether
/// the wording itself should change.
const NOW_PLAYING_LABEL: &str = "Music";

/// What the user is publishing, which decides how the status bar names it.
///
/// One value rather than a pair of labels: a publish that failed must not be described
/// with the word for one that succeeded, and passing "Posted" and "Note" separately
/// would let them drift apart at a call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublishKind {
    Note,
    Reaction,
    Repost,
}

impl PublishKind {
    /// How the bar names it once a relay has accepted it — past tense, and the wording
    /// the bar has always used.
    const fn settled_label(self) -> &'static str {
        match self {
            Self::Note => "Posted",
            Self::Reaction => "Reacted",
            Self::Repost => "Reposted",
        }
    }

    /// How the bar names it when it did not happen. Never the past tense: an error line
    /// reading "Posted" would claim exactly what this whole change exists to stop.
    const fn subject(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Reaction => "Reaction",
            Self::Repost => "Repost",
        }
    }
}

/// A publish handed to the worker and waiting for a relay's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPublish {
    kind: PublishKind,
    /// What was published, shown while pending and again once settled.
    message: String,
}

/// Status-bar label shown while a publish is waiting for a relay's answer.
const PENDING_LABEL: &str = "Sending";

/// Configuration state - holds all user-configurable settings
#[derive(Debug, Clone, Default)]
pub struct ConfigState {
    /// Current configuration loaded from file
    pub config: Config,
}

impl<'a> AppState<'a> {
    /// Initialize AppState with the specified public key
    pub fn new(current_user_pubkey: PublicKey) -> Self {
        Self {
            user: UserState::new_with_pubkey(current_user_pubkey),
            ..Default::default()
        }
    }

    /// Initialize AppState with the specified public key and config
    pub fn new_with_config(
        current_user_pubkey: PublicKey,
        config: Config,
        read_only: bool,
    ) -> Self {
        Self {
            user: UserState::new_with_pubkey(current_user_pubkey),
            config: ConfigState { config },
            read_only,
            ..Default::default()
        }
    }

    /// Process a received Nostr event for a specific tab
    pub fn process_nostr_event_for_tab(
        &mut self,
        event: Event,
        feed: &FeedKind,
    ) -> Command<AppMsg> {
        // Receiving any event means startup has produced its first results.
        self.startup.mark_completed();

        match event.kind {
            Kind::TextNote => {
                let current_loading_more_state = self.timeline.is_loading_more_for_feed(feed);

                let _ = self.timeline.update(TimelineMessage::NoteAddedToTab {
                    event,
                    feed: feed.clone(),
                });

                let new_loading_more_state = self.timeline.is_loading_more_for_feed(feed);

                if current_loading_more_state == Some(true) && new_loading_more_state == Some(false)
                {
                    let tab_title = self.active_tab_title();
                    self.set_status(tab_title, "loaded more");
                }

                Command::none()
            }
            Kind::Metadata => {
                // Metadata is shared across all tabs
                if let Ok(metadata) = Metadata::from_json(event.content.clone()) {
                    let profile = Profile::new(event.pubkey, event.created_at, metadata);
                    self.user.insert_newer_profile(profile);
                }
                Command::none()
            }
            Kind::Repost => {
                let _ = self.timeline.update(TimelineMessage::RepostAdded { event });
                Command::none()
            }
            Kind::Reaction => {
                let _ = self
                    .timeline
                    .update(TimelineMessage::ReactionAdded { event });
                Command::none()
            }
            Kind::ZapReceipt => {
                let _ = self
                    .timeline
                    .update(TimelineMessage::ZapReceiptAdded { event });
                Command::none()
            }
            _ => Command::none(),
        }
    }

    /// Open (or switch to) the mention timeline tab.
    ///
    /// If the Mention tab already exists, it is selected. Otherwise a new tab is
    /// created, a subscription is requested, and a "loading" status message is shown.
    pub fn open_mention_tab(&mut self) -> Command<AppMsg> {
        let feed = FeedKind::Mention;

        // Tab already open: just switch to it.
        if let Some(index) = self.timeline.find_tab_by_feed(&feed) {
            let _ = self.timeline.update(TimelineMessage::TabSelected { index });
            return Command::none();
        }

        let _ = self
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });

        if self.timeline.find_tab_by_feed(&feed).is_some() {
            log::info!("Created new mention timeline");

            let outcome = self
                .nostr
                .update(NostrMessage::SubscriptionRequested { feed });
            let _ = self.dispatch_nostr(outcome);

            self.set_status("Mention", "loading...");
        } else {
            log::error!("Failed to create mention timeline");

            self.set_status_error("Mention", "failed to open tab");
        }

        Command::none()
    }

    /// Open (or switch to) the author timeline tab for the given pubkey.
    ///
    /// If a tab for this author already exists, it is selected. Otherwise a new
    /// tab is created, a subscription is requested, and a "loading" status
    /// message is shown (or an error message if the tab could not be created).
    pub fn open_author_timeline(&mut self, author_pubkey: PublicKey) -> Command<AppMsg> {
        let Ok(author_npub) = author_pubkey.to_bech32();
        let feed = FeedKind::Author(author_pubkey);

        // Tab already open: just switch to it.
        if let Some(index) = self.timeline.find_tab_by_feed(&feed) {
            let _ = self.timeline.update(TimelineMessage::TabSelected { index });
            return Command::none();
        }

        // Otherwise create it, then subscribe and show the loading status.
        let _ = self
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });

        if self.timeline.find_tab_by_feed(&feed).is_some() {
            log::info!("Created new author timeline for {author_npub}");

            let outcome = self
                .nostr
                .update(NostrMessage::SubscriptionRequested { feed });
            let _ = self.dispatch_nostr(outcome);

            self.set_status(author_npub, "loading...");
        } else {
            log::error!("Failed to create author timeline");

            self.set_status_error(author_npub, "failed to open tab");
        }

        Command::none()
    }

    /// Close the currently active tab and unsubscribe from its subscriptions.
    ///
    /// The Home tab cannot be closed; the [`Timeline`] enforces this, so calling
    /// this while Home is active is a no-op apart from the (no-op) unsubscribe.
    pub fn close_current_tab(&mut self) -> Command<AppMsg> {
        let current_index = self.timeline.active_tab_index();

        // Capture the feed before removing the tab.
        let feed = self.timeline.active_tab().feed().clone();

        let _ = self.timeline.update(TimelineMessage::TabRemoved {
            index: current_index,
        });

        // Unsubscribe the subscriptions associated with the closed tab.
        let outcome = self.nostr.update(NostrMessage::SubscriptionClosed { feed });
        let _ = self.dispatch_nostr(outcome);

        Command::none()
    }

    /// Submit a NIP-25 reaction for the currently selected note.
    pub fn react_to_selected(&mut self) -> Command<AppMsg> {
        self.submit_engagement_for_selected(PublishKind::Reaction, TextNote::reaction_builder)
    }

    /// Submit a NIP-18 repost for the currently selected note.
    pub fn repost_selected(&mut self) -> Command<AppMsg> {
        self.submit_engagement_for_selected(PublishKind::Repost, TextNote::repost_builder)
    }

    /// Build an engagement event for the selected note with `build` and submit it.
    /// No-op when nothing is selected.
    fn submit_engagement_for_selected(
        &mut self,
        kind: PublishKind,
        build: fn(&TextNote) -> EventBuilder,
    ) -> Command<AppMsg> {
        let Some(note) = self.timeline.selected_note() else {
            // Redrawing, despite this arm doing nothing itself: `handle_timeline_msg`
            // clears the status bar before dispatching here, so by the time there is
            // nothing to submit the pass has already changed what is on screen.
            return Command::none();
        };

        let note_id = note.bech32_id();
        let event_builder = build(note);
        log::info!("{} event: {note_id}", kind.subject());

        let _ = self.publish(kind, note_id, event_builder);

        Command::none()
    }

    /// Start composing a reply to the currently selected note.
    ///
    /// Sets the reply context (target event and author profile) on the editor.
    /// No-op when nothing is selected.
    pub fn start_reply(&mut self) -> Command<AppMsg> {
        let Some(note) = self.timeline.selected_note() else {
            return Command::none();
        };

        let note_id = note.bech32_id();
        let event = note.as_event().clone();
        let author_pubkey = note.author_pubkey();
        log::info!("Starting reply to event: {note_id}");

        let profile = self.user.get_profile(&author_pubkey).cloned();

        self.editor.update(EditorMessage::ReplyStarted {
            to: Box::new(event),
            profile: Box::new(profile),
        });

        Command::none()
    }

    /// Publish the editor's current content as a text note, or as a NIP-10 reply
    /// when a reply target is set, then reset the editor.
    /// No-op when the composer is closed, and refused when the draft is blank.
    pub fn submit_note(&mut self) -> Command<AppMsg> {
        // A composer that is not open has nothing to submit, and trying costs more than
        // nothing: `ComposingCanceled` leaves the buffer alone — the next
        // `ComposingStarted` is what clears it — so a second submission arriving after
        // the first succeeded would read the same draft and publish the same note again.
        //
        // Guarded here rather than where the extra submission comes from, because that
        // is not one place: a key path can queue two before either is applied, and the
        // next way to do it need not be a key path at all (#538).
        if !self.editor.is_active() {
            log::warn!("Ignoring a note submission: the composer is not open");
            return Command::none().without_redraw();
        }

        let content = self.editor.get_content();

        // Refused before it costs anything. An empty note is a real event on the relays
        // that says nothing and cannot be recalled, and `Ctrl+P` on a composer nobody has
        // typed into is far likelier to be a slip than a request.
        //
        // Trimmed only to decide: whitespace and a stray newline are as empty as nothing
        // at all. What gets published is the content as typed.
        //
        // The composer stays open, like the pre-send refusals below, so nothing is lost
        // and the user can carry on typing — and it says so rather than ignoring the key,
        // which would read as a broken binding (#540).
        if content.trim().is_empty() {
            // At the level its neighbours use, not the one this deserves: "Ctrl+P did
            // nothing" gets triaged by grepping the log at warn and above, and a refusal
            // that only shows at info is missing from exactly that search. The bar is no
            // help by then — the next status has overwritten it.
            log::warn!("Refusing to publish a blank note");
            self.set_status_error(PublishKind::Note.subject(), "nothing to post");
            return Command::none();
        }

        let event_builder = if let Some(reply_to_event) = self.editor.reply_target() {
            log::info!("Publishing reply: {content}");
            // Build NIP-10 reply tags (root/reply markers, deduped p-tag).
            EventBuilder::new(Kind::TextNote, &content)
                .tags(ReplyTagsBuilder::build(reply_to_event.clone()))
        } else {
            log::info!("Publishing note: {content}");
            EventBuilder::new(Kind::TextNote, &content)
        };

        // Only discard the draft once it is actually on its way. Closing the editor
        // loses the text for good — the next `ComposingStarted` clears the buffer — and
        // this change is what makes a pre-send failure knowable in time to keep it. A
        // failure the relays report later still loses it: #514.
        if self.publish(PublishKind::Note, &content, event_builder) {
            self.editor.update(EditorMessage::ComposingCanceled);
        }

        Command::none()
    }

    /// Show the currently playing track, and broadcast it as a NIP-38 live status.
    /// No-op when the track is missing the fields required to build a status.
    ///
    /// The two halves are independent on purpose. The line says what is playing, which
    /// is true whether or not a relay ever hears about it; tying it to the broadcast
    /// would take the indicator away from anyone in read-only mode, and from anyone
    /// whose track changes before the worker is ready.
    pub fn publish_music_status(&mut self, track: Track) -> Command<AppMsg> {
        // Nothing was shown and nothing was sent, so this event needs no repaint. Not a
        // claim that the bar is then right: a rejected track arriving after a valid one
        // leaves the earlier `NOW_PLAYING_LABEL` line standing, along with the relay
        // status it published. Repainting would only draw that same stale text again — what to
        // do about superseding it is #521's.
        //
        // Not a rare path either: `MusicStatus::new` also rejects a track with no
        // duration, and radio and live streams routinely report none while changing
        // metadata as they play.
        let Some(status) = MusicStatus::new(track) else {
            return Command::none().without_redraw();
        };

        self.set_status(NOW_PLAYING_LABEL, status.content());

        // Broadcasting needs a key to sign with. Without one every send fails in the
        // worker, and #521 keeps that off the bar — so it would be a guaranteed failure,
        // on every track change, that the user is never told about.
        if self.read_only {
            return Command::none();
        }

        // Deliberately not tracked as a publish. Confirmation exists so the user is not
        // told their own action succeeded when it did not; a NIP-38 status is fired by a
        // track change, not by them, and there is nothing for them to do about a relay
        // refusing it. Routing it through the pending path would put an error on the bar
        // on every track change — many relays reject kind 30315 — for something they
        // never asked for. Whether the line should say more than "playing" is #521.
        let outcome = self.nostr.update(NostrMessage::EventSubmitted {
            id: None,
            event_builder: status.live_status_builder(),
        });
        let _ = self.dispatch_nostr(outcome);

        Command::none()
    }

    /// Title of the currently active tab, resolved against known profiles.
    fn active_tab_title(&self) -> String {
        self.timeline.active_tab().tab_title(self.user.profiles())
    }

    /// Show a status message in the status bar.
    fn set_status(&mut self, label: impl Into<String>, message: impl Into<String>) {
        self.status_bar.update(Message::MessageChanged {
            label: label.into(),
            message: message.into(),
        });
    }

    /// Show an error message in the status bar.
    fn set_status_error(&mut self, label: impl Into<String>, message: impl Into<String>) {
        self.status_bar.update(Message::ErrorMessageChanged {
            label: label.into(),
            message: message.into(),
        });
    }

    /// Dispatch a [`NostrOutcome`] produced by `model::nostr` to the worker.
    ///
    /// `model::nostr::update` is side-effect free and only reports the command
    /// to send; the application owns the sender and performs the actual I/O.
    #[must_use = "a command that never reached the worker will never be reported, and a \
                  publish left tracking it sits on \"Sending\" for good"]
    fn dispatch_nostr(&self, outcome: Option<NostrOutcome>) -> bool {
        let Some(NostrOutcome::Send(command)) = outcome else {
            // Not logged. Most `None`s are ordinary — `ConnectionReady` and
            // `SubscriptionCreated` never dispatch, `SubscriptionRequested` declines the
            // home feed and anything already subscribed — so warning here would fill the
            // log with false alarms on every successful startup. Where it matters, the
            // caller knows what it was trying to send and says so.
            return false;
        };

        let Some(sender) = self.command_sender.as_ref() else {
            log::warn!("Dropping Nostr command, worker not ready: {command:?}");
            return false;
        };

        if sender.send(command).is_err() {
            log::error!("Failed to send Nostr command: subscription worker is gone");
            return false;
        }

        true
    }

    /// Publish an event: track it, hand it to the worker, and settle it at once if the
    /// worker never got it.
    ///
    /// The three steps live together because they only make sense together — a tracked
    /// publish that was never dispatched sits on "Sending" for good, and a dispatch with
    /// nothing tracking it can never be settled by its report.
    ///
    /// Returns whether it is on its way. `false` means it already failed, and a caller
    /// about to discard what it published — the editor's buffer — should keep it.
    fn publish(
        &mut self,
        kind: PublishKind,
        message: impl Into<String>,
        event_builder: EventBuilder,
    ) -> bool {
        let id = self.begin_publish(kind, message);

        // Refused here rather than by the worker. Without a signing key every send fails,
        // so queueing it buys a round trip and a delayed error — and for a note it costs
        // the draft, because `submit_note` would have closed the editor by the time the
        // answer came back.
        if self.read_only {
            self.abandon_publish(id);
            return false;
        }

        let outcome = self.nostr.update(NostrMessage::EventSubmitted {
            id: Some(id),
            event_builder,
        });

        if !self.dispatch_nostr(outcome) {
            self.abandon_publish(id);
            return false;
        }

        true
    }

    /// Hand a submitted event to the worker and show it as pending.
    ///
    /// The bar says "Sending" rather than `settled_label` until a relay has answered,
    /// because until then nothing has been published — the command only sits on the
    /// worker's queue. [`Self::resolve_publish`] supplies the ending.
    ///
    /// A submission that never reaches the worker has to be settled instead, since no
    /// report will arrive for it — [`Self::publish`] does that, which is why the two are
    /// not called separately.
    fn begin_publish(&mut self, kind: PublishKind, message: impl Into<String>) -> PublishId {
        let id = PublishId(self.next_publish_id);
        self.next_publish_id = self.next_publish_id.wrapping_add(1);
        let message = message.into();

        // Nothing expires this. A worker that dies without reaching its exit drain — a
        // panic, an abort at quit — reports none of what it was holding, so every entry
        // outstanding at that moment stays and reads "Sending" until something else
        // writes the bar. Publishes issued *after* the death do settle themselves, since
        // they find the channel closed; it is the ones already tracked that strand, and
        // there can be several. Clearing them needs the application to notice the worker
        // died, which it cannot do today: #519.
        self.pending_publishes.insert(
            id,
            PendingPublish {
                kind,
                message: message.clone(),
            },
        );
        self.set_status(PENDING_LABEL, message);

        id
    }

    /// Settle the publish this outcome belongs to.
    ///
    /// An id with no entry is ignored: it can only mean the entry was already settled,
    /// and guessing which submission it meant is how a wrong note gets reported.
    pub fn resolve_publish(
        &mut self,
        id: PublishId,
        result: Result<(), String>,
    ) -> Command<AppMsg> {
        let Some(pending) = self.pending_publishes.remove(&id) else {
            // The id is the only thing that says which submission this was meant for.
            log::warn!("Publish outcome for an unknown submission {id:?}: {result:?}");
            return Command::none();
        };

        // Both arms write the bar whatever is on it. An answer can arrive a full ack
        // timeout after the submit, so this can land over a status the user has caused
        // since — a tab they opened while waiting. Deciding who may write over whom is
        // #516; doing it here would be the arbitration policy growing a case at a time,
        // which is what that issue exists to stop.
        match result {
            Ok(()) => self.set_status(pending.kind.settled_label(), pending.message),
            Err(reason) => {
                log::error!("Failed to publish {}: {reason}", pending.message);
                // Reason first: the bar is one line, and what a reaction or repost carries
                // as content is a bech32 id — long, and far less use than why it failed.
                self.set_status_error(
                    pending.kind.subject(),
                    format!("{reason} ({})", pending.message),
                );
            }
        }

        Command::none()
    }

    /// Give up on a publish that was never handed over, so it does not sit as "Sending".
    ///
    /// The cause is what the gateway believes, which is the most this layer can honestly
    /// say: a dispatch fails either because the gateway knows it is not connected, or
    /// because it thinks it is and the worker turned out to be gone. Both are worth
    /// telling the user, and neither claims more than is known.
    fn abandon_publish(&mut self, id: PublishId) {
        let Some(pending) = self.pending_publishes.remove(&id) else {
            return;
        };

        let cause = if self.read_only {
            "read-only mode"
        } else if self.nostr.is_ready() {
            "connection lost"
        } else {
            "not connected"
        };

        log::error!("Publish not sent, {cause}: {}", pending.message);
        self.set_status_error(
            pending.kind.subject(),
            format!("{cause} ({})", pending.message),
        );
    }

    /// Record that the Nostr subscription is ready and store its command sender,
    /// then show the initial "loading" status for the active tab.
    pub fn on_connection_ready(
        &mut self,
        command_sender: mpsc::UnboundedSender<NostrCommand>,
    ) -> Command<AppMsg> {
        log::info!("NostrEvents subscription ready");

        let tab_title = self.active_tab_title();
        self.command_sender = Some(command_sender);
        let outcome = self.nostr.update(NostrMessage::ConnectionReady);
        let _ = self.dispatch_nostr(outcome);
        self.set_status(tab_title, "loading...");

        Command::none()
    }

    /// Route an incoming relay event to the tab that owns its subscription.
    ///
    /// While still starting up, the first event flips the status to "loaded".
    /// Events whose subscription is not tracked by any tab are ignored.
    pub fn route_relay_event(
        &mut self,
        subscription_id: &SubscriptionId,
        event: Event,
    ) -> Command<AppMsg> {
        if self.startup.is_in_progress() {
            let tab_title = self.active_tab_title();
            self.set_status(tab_title, "loaded");
        }

        let Some(feed) = self
            .nostr
            .find_tab_by_subscription(subscription_id)
            .cloned()
        else {
            return Command::none();
        };

        log::debug!(
            "Routing event {} (kind: {:?}) to tab {feed:?}",
            event.id,
            event.kind
        );

        self.process_nostr_event_for_tab(event, &feed)
    }

    /// Request older events for the active tab, paginating before its oldest
    /// known timestamp. No-op when the active timeline has no events yet.
    pub fn load_more_timeline(&mut self) -> Command<AppMsg> {
        log::info!("Loading more timeline events");

        let Some(since) = self.timeline.oldest_timestamp() else {
            log::warn!("No oldest timestamp available, cannot load more");
            return Command::none();
        };

        let feed = self.timeline.active_tab().feed().clone();
        let tab_title = self.active_tab_title();

        let outcome = self
            .nostr
            .update(NostrMessage::HistoryRequested { feed, since });
        let _ = self.dispatch_nostr(outcome);

        self.set_status(tab_title, "loading more...");

        Command::none()
    }

    // --- Thin command methods ---
    //
    // These wrap a single sub-state transition so that `TearsApp` never mutates
    // a sub-state directly; all state changes flow through `AppState`. Read-only
    // access (used by the view) is intentionally left to the public fields.

    /// Clear the current status message.
    pub fn clear_status_message(&mut self) -> Command<AppMsg> {
        self.status_bar.update(Message::MessageCleared);
        Command::none()
    }

    /// Show a system-level error in the status bar.
    pub fn show_error(&mut self, error: String) -> Command<AppMsg> {
        log::error!("{error}");
        self.set_status_error("System", error);
        Command::none()
    }

    /// Move the selection to the previous timeline item.
    pub fn scroll_up(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::PreviousItemSelected);
        Command::none()
    }

    /// Move the selection to the next timeline item.
    ///
    /// When the selection is already at the bottom, the timeline reports
    /// `LoadMoreRequested` and the application loads older events.
    pub fn scroll_down(&mut self) -> Command<AppMsg> {
        match self.timeline.update(TimelineMessage::NextItemSelected) {
            Some(TimelineOutcome::LoadMoreRequested) => self.load_more_timeline(),
            None => Command::none(),
        }
    }

    /// Select the timeline item at `index`.
    pub fn select_note(&mut self, index: usize) -> Command<AppMsg> {
        let _ = self
            .timeline
            .update(TimelineMessage::ItemSelected { index });
        Command::none()
    }

    /// Clear the current timeline selection.
    pub fn deselect_note(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::ItemSelectionCleared);
        Command::none()
    }

    /// Select the first timeline item.
    pub fn select_first_note(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::FirstItemSelected);
        Command::none()
    }

    /// Select the last timeline item.
    pub fn select_last_note(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::LastItemSelected);
        Command::none()
    }

    /// Switch to the tab at `index`.
    pub fn select_tab(&mut self, index: usize) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::TabSelected { index });
        log::debug!("Selected tab index: {}", self.timeline.active_tab_index());
        Command::none()
    }

    /// Switch to the next tab (wraps around).
    pub fn next_tab(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::NextTabSelected);
        log::debug!("Switched to next tab: {}", self.timeline.active_tab_index());
        Command::none()
    }

    /// Switch to the previous tab (wraps around).
    pub fn prev_tab(&mut self) -> Command<AppMsg> {
        let _ = self.timeline.update(TimelineMessage::PreviousTabSelected);
        log::debug!(
            "Switched to previous tab: {}",
            self.timeline.active_tab_index()
        );
        Command::none()
    }

    /// Start composing a new note.
    pub fn start_composing(&mut self) -> Command<AppMsg> {
        self.editor.update(EditorMessage::ComposingStarted);
        Command::none()
    }

    /// Cancel the current composing/reply session.
    pub fn cancel_composing(&mut self) -> Command<AppMsg> {
        self.editor.update(EditorMessage::ComposingCanceled);
        Command::none()
    }

    /// Forward a key event to the editor's text area.
    pub fn process_text_input(&mut self, event: KeyEvent) -> Command<AppMsg> {
        self.editor
            .update(EditorMessage::KeyEventReceived { event });
        Command::none()
    }

    /// Close the Nostr connection: unsubscribe and disconnect from relays.
    pub fn close_connection(&mut self) -> Command<AppMsg> {
        let outcome = self.nostr.update(NostrMessage::ConnectionClosed);
        let _ = self.dispatch_nostr(outcome);
        self.command_sender = None;
        Command::none()
    }

    /// Track a subscription that the relay layer created for a tab.
    pub fn track_subscription_created(
        &mut self,
        feed: FeedKind,
        subscription_id: SubscriptionId,
    ) -> Command<AppMsg> {
        log::info!("Subscription created for {feed:?}: {subscription_id:?}");
        let outcome = self.nostr.update(NostrMessage::SubscriptionCreated {
            feed,
            sub_id: subscription_id,
        });
        let _ = self.dispatch_nostr(outcome);
        Command::none()
    }

    /// Show that the Nostr subscription was shut down.
    pub fn notify_subscription_shutdown(&mut self) -> Command<AppMsg> {
        log::info!("Nostr subscription shut down");
        self.set_status("Nostr", "disconnected");
        Command::none()
    }

    /// Show a Nostr subscription error in the status bar.
    pub fn notify_subscription_error(&mut self, error: CommandError) -> Command<AppMsg> {
        self.set_status_error("Nostr", format!("{error:?}"));
        Command::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{nostr::Profile, text::shorten_npub};
    use color_eyre::eyre::Result;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::Duration;

    fn create_track(title: &str) -> Track {
        Track {
            title: title.to_owned(),
            artist: vec!["Artist".to_owned()],
            duration: Some(Duration::from_secs(180)),
            album: None,
            album_artist: vec![],
            track_number: None,
            artwork: None,
        }
    }

    fn create_text_note(keys: &Keys, content: &str, created_at: Timestamp) -> Result<Event> {
        Ok(EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(created_at)
            .finalize(keys)?)
    }

    #[test]
    fn app_state_default() {
        let state = AppState::default();

        assert_eq!(state.timeline.len(), 0);
        assert!(!state.editor.is_active());
        assert!(state.startup.is_in_progress());
    }

    #[test]
    fn startup_mark_completed() {
        let mut startup = Startup::default();
        startup.mark_completed();
        assert!(!startup.is_in_progress());

        // mark_completed is idempotent
        startup.mark_completed();
        assert!(!startup.is_in_progress());
    }

    #[test]
    fn app_state_new_with_pubkey() {
        let keys = Keys::generate();
        let pubkey = keys.public_key();
        let state = AppState::new(pubkey);

        assert_eq!(state.user.current_user_pubkey(), pubkey);
        assert_eq!(state.timeline.len(), 0);
    }

    #[test]
    fn process_nostr_event_for_tab_text_note_routes_to_specified_tab() -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        // Add a user timeline tab first (before adding any events)
        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();
        let user_tab = FeedKind::Author(author_pubkey);
        let _ = state.timeline.update(TimelineMessage::TabAdded {
            feed: user_tab.clone(),
        });

        // Add event only to user timeline tab (this also stops loading)
        let event = create_text_note(&author_keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &user_tab);

        // Verify it was inserted only into the user timeline.
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });
        assert_eq!(state.timeline.len(), 0);

        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 1 });
        assert_eq!(state.timeline.len(), 1);

        // No loading_more => no status message update.
        assert_eq!(state.status_bar.message(), None);

        Ok(())
    }

    #[test]
    fn process_nostr_event_for_tab_propagates_timeline_command() -> Result<()> {
        let mut state = AppState::new(Keys::generate().public_key());

        let event = create_text_note(&Keys::generate(), "hello", Timestamp::from(1000))?;
        let command = state.process_nostr_event_for_tab(event, &FeedKind::Home);

        // The command returned by the timeline update is propagated to the caller
        // rather than discarded. Adding a note currently issues no follow-up command.
        assert!(command.is_none());

        Ok(())
    }

    #[test]
    fn process_nostr_event_for_tab_text_note_sets_status_when_load_more_completed_home(
    ) -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        // Use pre-loaded timeline for testing
        state.timeline = Timeline::default();

        // Ensure Home tab is active.
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });

        // Insert an initial note so oldest_timestamp exists.
        let keys = Keys::generate();
        let event1 = create_text_note(&keys, "newer", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event1, &FeedKind::Home);

        // Start loading more. (loading_more_since = oldest_timestamp = 1000)
        let _ = state.timeline.update(TimelineMessage::LastItemSelected);
        let _ = state.timeline.update(TimelineMessage::NextItemSelected);
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&FeedKind::Home),
            Some(true)
        );

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&keys, "older", Timestamp::from(500))?;
        let _ = state.process_nostr_event_for_tab(event2, &FeedKind::Home);

        assert_eq!(state.status_bar.message(), Some("[Home] loaded more"));
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&FeedKind::Home),
            Some(false)
        );

        Ok(())
    }

    #[test]
    fn process_nostr_event_for_tab_text_note_sets_status_when_load_more_completed_user_timeline(
    ) -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        // Use pre-loaded timeline for testing
        state.timeline = Timeline::default();

        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();
        let Ok(author_npub) = author_pubkey.to_bech32();
        let user_tab = FeedKind::Author(author_pubkey);
        let _ = state.timeline.update(TimelineMessage::TabAdded {
            feed: user_tab.clone(),
        });

        // Insert an initial note so oldest_timestamp exists.
        let event1 = create_text_note(&author_keys, "newer", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event1, &user_tab);

        let _ = state.timeline.update(TimelineMessage::LastItemSelected);
        let _ = state.timeline.update(TimelineMessage::NextItemSelected);
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&user_tab),
            Some(true)
        );

        // An older event completes the LoadMore operation.
        let event2 = create_text_note(&author_keys, "older", Timestamp::from(500))?;
        let _ = state.process_nostr_event_for_tab(event2, &user_tab);

        assert_eq!(
            state.status_bar.message(),
            Some(format!("[{}] loaded more", shorten_npub(author_npub)).as_ref())
        );
        assert_eq!(
            state.timeline.is_loading_more_for_feed(&user_tab),
            Some(false)
        );

        Ok(())
    }

    #[test]
    fn process_nostr_event_for_tab_metadata_inserts_profile_when_valid_json() -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();

        let metadata = Metadata::new().name("alice").display_name("Alice");
        let metadata_event = EventBuilder::new(Kind::Metadata, metadata.as_json())
            .custom_created_at(Timestamp::from(1000))
            .finalize(&author_keys)?;

        let _ = state.process_nostr_event_for_tab(metadata_event, &FeedKind::Home);

        let stored = state
            .user
            .get_profile(&author_pubkey)
            .expect("profile should be inserted");

        assert_eq!(
            stored,
            &Profile::new(author_pubkey, Timestamp::from(1000), metadata)
        );

        Ok(())
    }

    #[test]
    fn process_nostr_event_for_tab_metadata_ignores_invalid_json() -> Result<()> {
        let current_user_pubkey = Keys::generate().public_key();
        let mut state = AppState::new(current_user_pubkey);

        let author_keys = Keys::generate();
        let author_pubkey = author_keys.public_key();

        let invalid_metadata_event = EventBuilder::new(Kind::Metadata, "not json")
            .custom_created_at(Timestamp::from(1000))
            .finalize(&author_keys)?;

        let _ = state.process_nostr_event_for_tab(invalid_metadata_event, &FeedKind::Home);

        assert_eq!(state.user.get_profile(&author_pubkey), None);
        assert_eq!(state.user.profile_count(), 0);

        Ok(())
    }

    #[test]
    fn open_author_timeline_switches_to_existing_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let feed = FeedKind::Author(author_pubkey);

        // Pre-create the author tab, then move focus back to Home.
        let _ = state
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });
        assert_eq!(state.timeline.active_tab_index(), 0);

        let _ = state.open_author_timeline(author_pubkey);

        // Switches to the existing tab instead of creating a duplicate.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
    }

    #[test]
    fn open_author_timeline_creates_new_tab_and_shows_loading() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let Ok(author_npub) = author_pubkey.to_bech32();
        let feed = FeedKind::Author(author_pubkey);

        let _ = state.open_author_timeline(author_pubkey);

        // A new author tab is created, focused, and a loading status is shown.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[{author_npub}] loading...").as_str())
        );
    }

    #[test]
    fn open_mention_tab_switches_to_existing_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let feed = FeedKind::Mention;

        // Pre-create the mention tab, then move focus back to Home.
        let _ = state
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });
        let _ = state
            .timeline
            .update(TimelineMessage::TabSelected { index: 0 });
        assert_eq!(state.timeline.active_tab_index(), 0);

        let _ = state.open_mention_tab();

        // Switches to the existing tab instead of creating a duplicate.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
    }

    #[test]
    fn open_mention_tab_creates_new_tab_and_shows_loading() {
        let mut state = AppState::new(Keys::generate().public_key());
        let feed = FeedKind::Mention;

        let _ = state.open_mention_tab();

        // A new mention tab is created, focused, and a loading status is shown.
        assert_eq!(state.timeline.tabs().len(), 2);
        assert_eq!(state.timeline.active_tab().feed(), &feed);
        assert_eq!(state.status_bar.message(), Some("[Mention] loading..."));
    }

    #[test]
    fn close_current_tab_removes_active_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        let author_pubkey = Keys::generate().public_key();
        let feed = FeedKind::Author(author_pubkey);
        let _ = state.timeline.update(TimelineMessage::TabAdded { feed });
        assert_eq!(state.timeline.active_tab_index(), 1);

        let _ = state.close_current_tab();

        // The active author tab is removed and focus falls back to Home.
        assert_eq!(state.timeline.tabs().len(), 1);
        assert_eq!(state.timeline.active_tab().feed(), &FeedKind::Home);
    }

    #[test]
    fn close_current_tab_keeps_home_tab() {
        let mut state = AppState::new(Keys::generate().public_key());
        assert_eq!(state.timeline.active_tab().feed(), &FeedKind::Home);

        let _ = state.close_current_tab();

        // Home cannot be closed, so the timeline is unchanged.
        assert_eq!(state.timeline.tabs().len(), 1);
        assert_eq!(state.timeline.active_tab().feed(), &FeedKind::Home);
    }

    #[test]
    fn react_to_selected_without_selection_is_noop() {
        let mut state = AppState::new(Keys::generate().public_key());

        let command = state.react_to_selected();

        assert!(command.is_none());
        assert_eq!(state.status_bar.message(), None);
    }

    #[test]
    fn react_to_selected_sets_status() -> Result<()> {
        let (mut state, _rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let Ok(note1) = event.id.to_bech32();
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.react_to_selected();
        let id = only_pending(&state);

        // Handed to the worker; no relay has answered yet.
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Sending] {note1}").as_str())
        );

        let _ = state.resolve_publish(id, Ok(()));

        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Reacted] {note1}").as_str())
        );

        Ok(())
    }

    #[test]
    fn repost_selected_sets_status() -> Result<()> {
        let (mut state, _rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let Ok(note1) = event.id.to_bech32();
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.repost_selected();
        let id = only_pending(&state);

        // Handed to the worker; no relay has answered yet.
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Sending] {note1}").as_str())
        );

        let _ = state.resolve_publish(id, Ok(()));

        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Reposted] {note1}").as_str())
        );

        Ok(())
    }

    #[test]
    fn start_reply_without_selection_is_noop() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.start_reply();

        assert!(!state.editor.is_active());
        assert_eq!(state.editor.reply_target(), None);
    }

    #[test]
    fn start_reply_sets_reply_context() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event.clone(), &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.start_reply();

        assert!(state.editor.is_active());
        assert_eq!(state.editor.reply_target(), Some(&event));

        Ok(())
    }

    #[test]
    fn submit_note_posts_content_and_resets_editor() {
        let (mut state, _rx) = connected_state();

        // Compose "hi" in the editor.
        state.editor.update(EditorMessage::ComposingStarted);
        for code in ["h", "i"] {
            state.editor.update(EditorMessage::KeyEventReceived {
                event: KeyEvent::new(
                    KeyCode::Char(code.chars().next().expect("single char")),
                    KeyModifiers::NONE,
                ),
            });
        }

        let _ = state.submit_note();
        let id = only_pending(&state);

        // The editor closes, but nothing is posted until a relay says so.
        assert_eq!(state.status_bar.message(), Some("[Sending] hi"));
        assert!(!state.editor.is_active());

        let _ = state.resolve_publish(id, Ok(()));

        assert_eq!(state.status_bar.message(), Some("[Posted] hi"));
    }

    /// #540: an empty note is an event the relays keep and nobody can read.
    #[test]
    fn submit_note_refuses_an_empty_draft() {
        let (mut state, _rx) = connected_state();

        state.editor.update(EditorMessage::ComposingStarted);

        let _ = state.submit_note();

        assert!(state.pending_publishes.is_empty(), "nothing was published");
        assert_eq!(
            state.status_bar.message(),
            Some("[ERR: Note] nothing to post")
        );
        assert!(state.editor.is_active(), "the composer stays open");
    }

    /// Whitespace and a stray newline are as empty as nothing at all.
    ///
    /// Both shapes, because they are not the same one. Spaces leave a line holding
    /// whitespace; pressing Enter on an untouched composer leaves two empty lines, which
    /// `get_content` joins into a bare `"\n"`. Same answer, different buffer.
    #[test]
    fn submit_note_refuses_a_draft_of_only_whitespace() {
        for keys in [
            vec![KeyCode::Char(' '), KeyCode::Char(' ')],
            vec![KeyCode::Enter],
        ] {
            let (mut state, _rx) = connected_state();

            state.editor.update(EditorMessage::ComposingStarted);
            for code in &keys {
                state.editor.update(EditorMessage::KeyEventReceived {
                    event: KeyEvent::new(*code, KeyModifiers::NONE),
                });
            }

            // Otherwise this would hold just as well if the keystrokes never landed, and
            // would be saying "an empty buffer is empty" rather than what it claims.
            assert!(
                !state.editor.get_content().is_empty(),
                "{keys:?} should have reached the buffer"
            );

            let _ = state.submit_note();

            assert!(
                state.pending_publishes.is_empty(),
                "{keys:?} should have published nothing"
            );
            assert_eq!(
                state.status_bar.message(),
                Some("[ERR: Note] nothing to post")
            );
            assert!(state.editor.is_active(), "the composer stays open");
        }
    }

    /// The other side of it: trimming decides, and does not touch what is sent.
    ///
    /// Asserted on the event handed to the worker, not only on the status line. Both are
    /// built from the same `content`, so a bar reading `[Sending]  hi ` would go on
    /// reading that if the builder started trimming — which is the one change this test
    /// exists to catch. The bar is checked too, since it is what the user sees.
    #[test]
    fn submit_note_posts_padded_content_as_typed() {
        let (mut state, mut rx) = connected_state();

        state.editor.update(EditorMessage::ComposingStarted);
        for code in [' ', 'h', 'i', ' '] {
            state.editor.update(EditorMessage::KeyEventReceived {
                event: KeyEvent::new(KeyCode::Char(code), KeyModifiers::NONE),
            });
        }

        let _ = state.submit_note();

        let Ok(NostrCommand::SendEventBuilder { event_builder, .. }) = rx.try_recv() else {
            panic!("the note should have been handed to the worker");
        };
        let event = event_builder
            .finalize(&Keys::generate())
            .expect("the builder should sign");

        assert_eq!(event.content, " hi ");
        assert_eq!(state.status_bar.message(), Some("[Sending]  hi "));
    }

    /// #538: the buffer outlives the composer, so a submission that arrives after the
    /// editor closed would publish the same note a second time.
    #[test]
    fn submit_note_publishes_once_when_submitted_twice() {
        let (mut state, _rx) = connected_state();

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();
        let id = only_pending(&state);

        // The composer is closed now, and the draft is still in the buffer.
        assert!(!state.editor.is_active());
        assert_eq!(state.editor.get_content(), "h");

        let _ = state.submit_note();

        assert_eq!(
            only_pending(&state),
            id,
            "the second submission published nothing"
        );
        assert_eq!(state.status_bar.message(), Some("[Sending] h"));
    }

    #[test]
    fn submit_note_as_reply_posts_and_resets_editor() -> Result<()> {
        let (mut state, _rx) = connected_state();
        let keys = Keys::generate();

        // Select a note and start replying to it.
        let event = create_text_note(&keys, "original", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);
        let _ = state.start_reply();

        // Type a reply and submit it (exercises the NIP-10 reply branch).
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();
        let id = only_pending(&state);

        assert_eq!(state.status_bar.message(), Some("[Sending] y"));
        assert!(!state.editor.is_active());

        let _ = state.resolve_publish(id, Ok(()));

        assert_eq!(state.status_bar.message(), Some("[Posted] y"));

        Ok(())
    }

    #[test]
    fn publish_music_status_sets_status() {
        let (mut state, _rx) = connected_state();

        let _ = state.publish_music_status(create_track("Song"));

        // Not tracked as a publish: a track change is not something the user did, so the
        // line reports what nostui attempted, as it always has (#521).
        assert_eq!(state.status_bar.message(), Some("[Music] Song - Artist"));
        assert!(state.pending_publishes.is_empty());
    }

    #[test]
    fn read_only_mode_refuses_a_note_and_keeps_the_draft() {
        // Connected, but with no key to sign with — so the send would fail in the worker,
        // by which time the editor would have been closed and the text lost.
        let mut state =
            AppState::new_with_config(Keys::generate().public_key(), Config::default(), true);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _ = state.on_connection_ready(tx);
        while rx.try_recv().is_ok() {}

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();

        assert!(state.editor.is_active());
        assert_eq!(state.editor.get_content(), "h");
        assert_eq!(
            state.status_bar.message(),
            Some("[ERR: Note] read-only mode (h)")
        );

        // Nothing was queued, so no report will arrive to settle a phantom entry.
        assert!(rx.try_recv().is_err());
        assert!(state.pending_publishes.is_empty());
    }

    #[test]
    fn read_only_mode_does_not_broadcast_a_music_status() {
        // An `npub` configuration: connected, but with no key to sign with.
        let mut state =
            AppState::new_with_config(Keys::generate().public_key(), Config::default(), true);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _ = state.on_connection_ready(tx);
        while rx.try_recv().is_ok() {}

        let _ = state.publish_music_status(create_track("Song"));

        // Nothing is sent: every send would fail in the worker, and #521 keeps that off
        // the bar, so it would be a guaranteed failure the user never sees.
        assert!(rx.try_recv().is_err());

        // The indicator is not a casualty of that. It says what is playing, which is as
        // true without a signing key as with one.
        assert_eq!(state.status_bar.message(), Some("[Music] Song - Artist"));
    }

    #[test]
    fn the_now_playing_line_does_not_depend_on_reaching_a_relay() {
        // Not connected, so the broadcast is declined before it reaches the worker.
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.publish_music_status(create_track("Song"));

        // The line says what is playing, which is true either way. Tying it to the
        // broadcast would take the indicator away whenever a track changes before the
        // worker is ready — likely at startup, since the two come up independently.
        assert_eq!(state.status_bar.message(), Some("[Music] Song - Artist"));
        assert!(state.pending_publishes.is_empty());
    }

    #[test]
    fn publish_music_status_ignores_invalid_track() {
        let mut state = AppState::new(Keys::generate().public_key());

        // A track with an empty title cannot form a status, so nothing happens.
        let _ = state.publish_music_status(create_track(""));

        assert_eq!(state.status_bar.message(), None);
    }

    #[test]
    fn on_connection_ready_marks_ready_and_shows_loading() {
        let mut state = AppState::new(Keys::generate().public_key());
        let (tx, _rx) = mpsc::unbounded_channel();

        let _ = state.on_connection_ready(tx);

        assert!(state.nostr.is_ready());
        assert_eq!(state.status_bar.message(), Some("[Home] loading..."));
    }

    #[test]
    fn route_relay_event_routes_to_owning_tab() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        // Associate a subscription with the Home tab.
        let sub_id = SubscriptionId::new("home_sub");
        let _ = state.nostr.update(NostrMessage::SubscriptionCreated {
            feed: FeedKind::Home,
            sub_id: sub_id.clone(),
        });

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.route_relay_event(&sub_id, event);

        // The event is routed to the Home tab, and the first event ends startup.
        assert_eq!(state.timeline.len(), 1);
        assert!(!state.startup.is_in_progress());
        assert_eq!(state.status_bar.message(), Some("[Home] loaded"));

        Ok(())
    }

    #[test]
    fn load_more_timeline_without_events_is_noop() {
        let mut state = AppState::new(Keys::generate().public_key());

        // No events => no oldest timestamp => nothing to paginate.
        let _ = state.load_more_timeline();

        assert_eq!(state.status_bar.message(), None);
    }

    #[test]
    fn load_more_timeline_sets_loading_status() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);

        let _ = state.load_more_timeline();

        assert_eq!(state.status_bar.message(), Some("[Home] loading more..."));

        Ok(())
    }

    #[test]
    fn route_relay_event_ignores_untracked_subscription() -> Result<()> {
        let keys = Keys::generate();
        let mut state = AppState::new(keys.public_key());

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.route_relay_event(&SubscriptionId::new("unknown"), event);

        // No tab owns the subscription, so the event is dropped, but the
        // "loaded" status is still shown while starting up.
        assert_eq!(state.timeline.len(), 0);
        assert_eq!(state.status_bar.message(), Some("[Home] loaded"));

        Ok(())
    }

    // --- Dispatch seam: each use case sends the expected NostrCommand ---
    //
    // The model only emits commands once connected, and the application owns the
    // sender, so these tests connect via `on_connection_ready` (which injects the
    // sender and marks the worker ready) and then drain the receiver. They guard
    // the application <-> worker wiring that visible-side-effect tests miss — e.g.
    // the #458 regression where `load_more_timeline` set the status but never
    // dispatched `LoadMore`.

    /// The id of the one outstanding publish, for tests that settle it by hand.
    fn only_pending(state: &AppState<'_>) -> PublishId {
        let mut ids = state.pending_publishes.keys();
        let id = *ids.next().expect("a publish should be pending");
        assert!(ids.next().is_none(), "expected exactly one pending publish");
        id
    }

    /// A connected state with a note selected and a reaction to it in flight.
    ///
    /// A reaction stands in for "something the user did". Those are the publishes this
    /// change confirms; a NIP-38 status is not one of them.
    fn state_with_a_pending_reaction() -> (
        AppState<'static>,
        mpsc::UnboundedReceiver<NostrCommand>,
        PublishId,
        String,
    ) {
        let (mut state, mut rx) = connected_state();
        let keys = Keys::generate();

        let event =
            create_text_note(&keys, "hello", Timestamp::from(1000)).expect("a valid text note");
        let Ok(note_id) = event.id.to_bech32();
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);
        while rx.try_recv().is_ok() {}

        let _ = state.react_to_selected();
        let id = only_pending(&state);
        (state, rx, id, note_id)
    }

    fn connected_state() -> (AppState<'static>, mpsc::UnboundedReceiver<NostrCommand>) {
        let mut state = AppState::new(Keys::generate().public_key());
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = state.on_connection_ready(tx);
        (state, rx)
    }

    #[test]
    fn publish_failure_reports_an_error_instead_of_success() {
        let (mut state, _rx, id, note_id) = state_with_a_pending_reaction();

        let _ = state.resolve_publish(
            id,
            Err(String::from(
                "no relay confirmed the event: wss://relay.example: blocked",
            )),
        );

        let message = state.status_bar.message().expect("a status message");
        assert!(
            message.starts_with("[ERR: Reaction] no relay confirmed") && message.contains(&note_id),
            "expected the reason and the content, got: {message}"
        );
    }

    #[test]
    fn a_submission_that_fails_before_it_is_sent_keeps_the_draft() {
        // Not connected, so the failure is known before the command is queued.
        let mut state = AppState::new(Keys::generate().public_key());

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();

        // Closing the editor loses the text for good: the next `ComposingStarted` clears
        // the buffer, so there is no way back to it.
        assert!(state.editor.is_active());
        assert_eq!(state.editor.get_content(), "h");
    }

    /// The other side of #538's guard: it must not block the retry the kept draft
    /// exists for. A submission that failed before it was sent leaves the composer
    /// open, so the next one is a first attempt, not a duplicate.
    #[test]
    fn a_kept_draft_can_still_be_submitted_again() {
        let mut state = AppState::new(Keys::generate().public_key());

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        // Not connected: the failure is known before the command is queued.
        let _ = state.submit_note();
        assert!(state.editor.is_active());
        assert!(state.pending_publishes.is_empty());

        let (tx, _rx) = mpsc::unbounded_channel();
        let _ = state.on_connection_ready(tx);
        let _ = state.submit_note();

        let _ = only_pending(&state);
        assert_eq!(state.status_bar.message(), Some("[Sending] h"));
        assert!(!state.editor.is_active());
    }

    #[test]
    fn a_submission_that_is_on_its_way_closes_the_editor() {
        let (mut state, _rx) = connected_state();

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();

        assert!(!state.editor.is_active());
    }

    #[test]
    fn a_failed_note_is_not_called_posted() {
        let (mut state, _rx) = connected_state();

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();
        let id = only_pending(&state);

        let _ = state.resolve_publish(id, Err(String::from("refused")));

        // `[ERR: Posted]` for a note no relay took would be the same false claim this
        // whole change removes, just in a different tense.
        let message = state.status_bar.message().expect("a status message");
        assert!(
            message.starts_with("[ERR: Note]"),
            "a failure must not be named with the word for a success, got: {message}"
        );
        assert!(!message.contains("Posted"), "got: {message}");
    }

    #[test]
    fn a_publish_is_settled_when_the_worker_has_gone() {
        let (mut state, rx) = connected_state();

        // The gateway still believes it is connected — this is the other way a dispatch
        // fails, and the one the disconnected test cannot reach.
        drop(rx);

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });
        let _ = state.submit_note();

        // The gateway still believed it was connected, so this is a lost connection
        // rather than never having had one — the two are worth telling apart.
        assert_eq!(
            state.status_bar.message(),
            Some("[ERR: Note] connection lost (h)")
        );

        // Nothing is left tracking it. An entry here would sit on "Sending" for good,
        // since the worker that would have reported it is the one that went away.
        assert!(state.pending_publishes.is_empty());
    }

    #[test]
    fn publishing_while_disconnected_does_not_claim_success() {
        // Not connected: `model::nostr` declines the submission, so nothing is queued and
        // no outcome will ever arrive to settle it.
        let mut state = AppState::new(Keys::generate().public_key());

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });
        let _ = state.submit_note();

        assert_eq!(
            state.status_bar.message(),
            Some("[ERR: Note] not connected (h)")
        );
        assert!(state.pending_publishes.is_empty());
    }

    #[test]
    fn the_dispatched_command_carries_the_id_the_publish_is_tracked_under() {
        let (_state, mut rx, tracked, _note_id) = state_with_a_pending_reaction();

        let Ok(NostrCommand::SendEventBuilder { id: sent, .. }) = rx.try_recv() else {
            panic!("a publish should have been dispatched");
        };

        // The two halves of the correlation. If they ever disagree, no report can find
        // its submission and every publish strands on "Sending" — silently, since each
        // half is individually plausible.
        assert_eq!(sent, Some(tracked));
    }

    #[test]
    fn outcomes_settle_the_submission_they_belong_to() -> Result<()> {
        let (mut state, _rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let Ok(note1) = event.id.to_bech32();
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        // Two outstanding at once.
        let _ = state.react_to_selected();
        let reaction = only_pending(&state);
        let _ = state.repost_selected();

        // Answered out of order — which is the point of correlating rather than queueing.
        let repost = *state
            .pending_publishes
            .keys()
            .find(|id| **id != reaction)
            .expect("two pending publishes");
        let _ = state.resolve_publish(repost, Ok(()));
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Reposted] {note1}").as_str())
        );

        let _ = state.resolve_publish(reaction, Ok(()));
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Reacted] {note1}").as_str())
        );

        Ok(())
    }

    #[test]
    fn an_outcome_for_an_unknown_submission_is_ignored() {
        let (mut state, _rx) = connected_state();
        let before = state.status_bar.message().map(ToOwned::to_owned);

        // Nothing is pending under this id, so there is nothing it could honestly settle.
        let _ = state.resolve_publish(PublishId(99), Ok(()));

        assert_eq!(state.status_bar.message(), before.as_deref());
    }

    #[test]
    fn a_publish_settles_only_once() {
        let (mut state, _rx, id, note_id) = state_with_a_pending_reaction();

        let _ = state.resolve_publish(id, Ok(()));
        assert_eq!(
            state.status_bar.message(),
            Some(format!("[Reacted] {note_id}").as_str())
        );

        // A duplicate report must not re-settle it over whatever is on screen by then.
        let _ = state.clear_status_message();
        let _ = state.resolve_publish(id, Ok(()));
        assert_eq!(state.status_bar.message(), None);
    }

    #[test]
    fn on_connection_ready_dispatches_nothing() {
        let (_state, mut rx) = connected_state();

        // Becoming ready must not, by itself, send any command.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn open_author_timeline_dispatches_subscribe() {
        let (mut state, mut rx) = connected_state();
        let author_pubkey = Keys::generate().public_key();

        let _ = state.open_author_timeline(author_pubkey);

        assert_eq!(
            rx.try_recv(),
            Ok(NostrCommand::Subscribe {
                feed: FeedKind::Author(author_pubkey),
            })
        );
    }

    #[test]
    fn close_current_tab_dispatches_unsubscribe() {
        let (mut state, mut rx) = connected_state();
        let feed = FeedKind::Author(Keys::generate().public_key());
        let sub_id = SubscriptionId::new("author_sub");

        // Open an author tab and register a subscription for it without going
        // through `open_author_timeline` (which would also emit `Subscribe`).
        let _ = state
            .timeline
            .update(TimelineMessage::TabAdded { feed: feed.clone() });
        let _ = state.track_subscription_created(feed, sub_id.clone());

        let _ = state.close_current_tab();

        assert_eq!(
            rx.try_recv(),
            Ok(NostrCommand::Unsubscribe {
                subscription_ids: vec![sub_id],
            })
        );
    }

    #[test]
    fn react_to_selected_dispatches_send_event() -> Result<()> {
        let (mut state, mut rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.react_to_selected();

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));

        Ok(())
    }

    #[test]
    fn repost_selected_dispatches_send_event() -> Result<()> {
        let (mut state, mut rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);
        let _ = state.timeline.update(TimelineMessage::FirstItemSelected);

        let _ = state.repost_selected();

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));

        Ok(())
    }

    #[test]
    fn submit_note_dispatches_send_event() {
        let (mut state, mut rx) = connected_state();

        state.editor.update(EditorMessage::ComposingStarted);
        state.editor.update(EditorMessage::KeyEventReceived {
            event: KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE),
        });

        let _ = state.submit_note();

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));
    }

    #[test]
    fn publish_music_status_dispatches_send_event() {
        let (mut state, mut rx) = connected_state();

        let _ = state.publish_music_status(create_track("Song"));

        assert!(matches!(
            rx.try_recv(),
            Ok(NostrCommand::SendEventBuilder { .. })
        ));
    }

    #[test]
    fn load_more_timeline_dispatches_load_more() -> Result<()> {
        // Regression guard for #458: the status-only test passed while the
        // `LoadMore` dispatch was missing.
        let (mut state, mut rx) = connected_state();
        let keys = Keys::generate();

        let event = create_text_note(&keys, "hello", Timestamp::from(1000))?;
        let _ = state.process_nostr_event_for_tab(event, &FeedKind::Home);

        let _ = state.load_more_timeline();

        assert_eq!(
            rx.try_recv(),
            Ok(NostrCommand::LoadMore {
                feed: FeedKind::Home,
                since: Timestamp::from(1000),
            })
        );

        Ok(())
    }

    #[test]
    fn close_connection_dispatches_shutdown() {
        let (mut state, mut rx) = connected_state();

        let _ = state.close_connection();

        assert_eq!(rx.try_recv(), Ok(NostrCommand::Shutdown));
    }

    #[test]
    fn show_error_sets_error_status() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.show_error("boom".to_owned());

        assert_eq!(state.status_bar.message(), Some("[ERR: System] boom"));
    }

    #[test]
    fn notify_subscription_error_sets_error_status() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.notify_subscription_error(CommandError::AddRelayFailed {
            url: String::from("wss://relay.example"),
            error: "x".to_owned(),
        });

        let message = state.status_bar.message().expect("status set");
        assert!(message.starts_with("[ERR: Nostr]"));
    }

    /// The word reaches the user, and `typos` scans this file but does not know
    /// `disconntected` — this very comment carries that spelling and still lints
    /// clean. So the spelling is asserted here instead of left to the linter.
    #[test]
    fn notify_subscription_shutdown_says_disconnected() {
        let mut state = AppState::new(Keys::generate().public_key());

        let _ = state.notify_subscription_shutdown();

        assert_eq!(state.status_bar.message(), Some("[Nostr] disconnected"));
    }
}
