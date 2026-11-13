// The Swift milestone toast's behavior and capsule. Its entrance and bear's
// wave are added by the motion port after Mike has reviewed its clips.
use crate::app::Threadbear;
use crate::style::{self, px};
use crate::tips::Tip as _;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::time::{Duration, Instant};
use threadbear_core::milestones::{self as rules, Decision};

#[derive(Default)]
pub(crate) struct Milestones {
    pub(crate) note: Option<usize>,
    pub(crate) active: bool,
    pending: bool,
    last_count: Option<usize>,
    #[cfg(feature = "automation")]
    pub(crate) count: Option<usize>,
}

impl Milestones {
    pub(crate) fn new(active: bool) -> Self { Self { active, ..Default::default() } }
}

impl Threadbear {
    pub(crate) fn consider_milestone(&mut self, cx: &mut Context<Self>) {
        #[cfg(feature = "automation")]
        let scripted = self.milestone.count.is_some();
        #[cfg(not(feature = "automation"))]
        let scripted = false;
        if !scripted && (!crate::installed::launch().installed || !crate::installed::on_real_data()) { return; }
        if !self.sidebar.loaded || !self.remote.heard || self.milestone.pending || self.milestone.note.is_some() { return; }
        if self.milestone.last_count == Some(self.threads.len()) { return; }
        self.milestone.last_count = Some(self.threads.len());
        match rules::decide(self.milestone_count(), self.settings.saved.thread_milestone_shown) {
            Decision::Nothing => {}
            Decision::Remember(tier) => {
                self.settings.saved.thread_milestone_shown = Some(tier);
                self.settings.save();
            }
            Decision::Show(_) => {
                self.milestone.pending = true;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(400)).await;
                    let mut visible = None::<Instant>;
                    loop {
                        let keep = this.update(cx, |this, cx| {
                            let idle = this.milestone_idle_seconds();
                            let here = this.milestone.active && idle < 60.;
                            if let Some(since) = visible {
                                if this.milestone.note.is_none() { this.milestone.pending = false; return false; }
                                if since.elapsed() >= Duration::from_secs(5) && here && idle <= 5. && !this.toast_hovered {
                                    this.milestone.note = None;
                                    this.milestone.pending = false;
                                    cx.notify();
                                    return false;
                                }
                                return true;
                            }
                            if !here || this.archived_note.is_some() || this.inbox_end_note.is_some() || this.import_note.is_some() { return true; }
                            let Decision::Show(tier) = rules::decide(this.milestone_count(), this.settings.saved.thread_milestone_shown) else {
                                this.milestone.pending = false;
                                return false;
                            };
                            this.settings.saved.thread_milestone_shown = Some(tier);
                            this.settings.save();
                            this.milestone.note = Some(tier);
                            visible = Some(Instant::now());
                            cx.notify();
                            true
                        }).unwrap_or(false);
                        if !keep { break; }
                        cx.background_executor().timer(Duration::from_millis(300)).await;
                    }
                }).detach();
            }
        }
    }

    fn milestone_count(&self) -> usize {
        #[cfg(feature = "automation")]
        if let Some(count) = self.milestone.count { return count; }
        self.threads.values().filter(|t| t.sample != Some(true)).count()
    }

    fn milestone_idle_seconds(&self) -> f64 {
        #[cfg(feature = "automation")]
        if self.milestone.count.is_some() { return 0.; }
        crate::app_update::idle_seconds()
    }

    pub(crate) fn render_milestone_toast(&self, tier: usize, cx: &mut Context<Self>) -> AnyElement {
        self.toast_capsule("milestone-toast", cx)
            .gap(px(7.))
            .test_support().aria_label("Dismiss milestone")
            .tip("Dismiss milestone")
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.milestone.note = None;
                cx.notify();
            }))
            .child(milestone_bear(cx))
            .child(div().id("milestone-title").test_support().aria_label(rules::title(tier)).child(rules::title(tier)))
            .child(div().text_color(style::secondary_text(cx)).child(rules::line(tier)))
            .into_any_element()
    }
}

/// The motion port's hook for the Swift toast's 17pt waving bear.
pub(crate) fn milestone_bear(_cx: &App) -> AnyElement {
    div().w(px(17.)).h(px(17.)).into_any_element()
}

#[cfg(feature = "automation")]
pub(crate) fn automation(app: &Entity<Threadbear>, cmd: &serde_json::Value, cx: &mut App) -> serde_json::Value {
    app.update(cx, |this, cx| {
        this.milestone.count = cmd["count"].as_u64().map(|n| n as usize);
        this.milestone.active = cmd["active"].as_bool().unwrap_or(true);
        this.milestone.last_count = None;
        if let Some(shown) = cmd["shown"].as_u64() { this.settings.saved.thread_milestone_shown = Some(shown as usize); }
        this.consider_milestone(cx);
    });
    serde_json::json!("ok milestone")
}
