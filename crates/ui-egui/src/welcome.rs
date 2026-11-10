//! The welcome screen, laid out as VEGAS 23's (File › Welcome Screen, and
//! when BananaV starts): New Project — a banner, the shape of the video
//! (widescreen, fullscreen, portrait, square, scope), its resolution and
//! frame rate, then Create Project or Advanced Settings… (Project
//! Properties); Open Project — Browse… and the recent projects; Getting
//! Started — BananaV's videos, site, community and social media (to come);
//! and Language, BananaV's own, for whoever opens it the first time. "Show
//! Welcome Screen on Startup" at the bottom.

use bv_core::FrameRate;

use crate::app::{BananaApp, Pending};
use crate::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, UiBuilder, pos2, vec2,
};
use crate::i18n::{Language, tr};
use crate::icons::{self, Icon};
use crate::theme;

/// The welcome screen's size, the side bar's width and the header's and
/// footer's heights.
const SIZE: egui::Vec2 = vec2(830.0, 556.0);
const SIDE_W: f32 = 100.0;
const HEADER_H: f32 = 46.0;
const FOOTER_H: f32 = 30.0;
/// The New Project banner's height and the shape buttons'.
const BANNER_H: f32 = 252.0;
const SHAPE_H: f32 = 72.0;

/// The pages of the side bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    New,
    Open,
    GettingStarted,
    Language,
}

impl Page {
    const ALL: [Page; 4] = [Page::New, Page::Open, Page::GettingStarted, Page::Language];

    fn name(self) -> &'static str {
        match self {
            Page::New => tr("New Project"),
            Page::Open => tr("Open Project"),
            Page::GettingStarted => tr("Getting Started"),
            Page::Language => tr("Language"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::New => Icon::File,
            Page::Open => Icon::Folder,
            Page::GettingStarted => Icon::Mortarboard,
            Page::Language => Icon::Globe,
        }
    }
}

/// The shape of a new project's frame, as VEGAS's welcome screen offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Shape {
    #[default]
    Widescreen,
    Fullscreen,
    Portrait,
    Square,
    Scope,
}

impl Shape {
    const ALL: [Shape; 5] = [
        Shape::Widescreen,
        Shape::Fullscreen,
        Shape::Portrait,
        Shape::Square,
        Shape::Scope,
    ];

    fn name(self) -> &'static str {
        match self {
            Shape::Widescreen => tr("Widescreen (16:9)"),
            Shape::Fullscreen => tr("Fullscreen (4:3)"),
            Shape::Portrait => tr("Portrait (9:16)"),
            Shape::Square => tr("Square (1:1)"),
            Shape::Scope => tr("Scope (2.4:1)"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Shape::Widescreen => Icon::ShapeWide,
            Shape::Fullscreen => Icon::ShapeFull,
            Shape::Portrait => Icon::ShapePortrait,
            Shape::Square => Icon::ShapeSquare,
            Shape::Scope => Icon::ShapeScope,
        }
    }

    /// The frame sizes of HD, FullHD and UHD.
    fn sizes(self) -> [(u32, u32); 3] {
        match self {
            Shape::Widescreen => [(1280, 720), (1920, 1080), (3840, 2160)],
            Shape::Fullscreen => [(1280, 960), (1920, 1440), (4096, 3072)],
            Shape::Portrait => [(720, 1280), (1080, 1920), (2160, 3840)],
            Shape::Square => [(720, 720), (1080, 1080), (2160, 2160)],
            Shape::Scope => [(1280, 536), (2048, 858), (4096, 1716)],
        }
    }
}

/// The resolutions' names (names: not translated).
const SIZES: [&str; 3] = ["HD", "FullHD", "UHD"];

/// The frame rates offered, in frames a second (1/1.001 of them with Drop
/// Frame, as VEGAS offers them).
const RATES: [u32; 7] = [24, 25, 30, 48, 50, 60, 120];

/// The frame rate `n` frames a second, or 1/1.001 of it.
fn frame_rate(n: u32, drop_frame: bool) -> FrameRate {
    if drop_frame {
        FrameRate::new(n * 1000, 1001)
    } else {
        FrameRate::new(n, 1)
    }
}

/// "30p (29.970)".
fn rate_label(n: u32, drop_frame: bool) -> String {
    let r = frame_rate(n, drop_frame).as_f64();
    format!("{n}p ({})", crate::fmt::num(r, 3))
}

/// Getting Started's tabs: where BananaV's videos, help, community and
/// social media will be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Channel {
    #[default]
    Website,
    GitHub,
    Discord,
    Donate,
}

impl Channel {
    const ALL: [Channel; 4] = [
        Channel::Website,
        Channel::GitHub,
        Channel::Discord,
        Channel::Donate,
    ];

    /// Its tab's name (GitHub and Discord are names in every language).
    fn name(self) -> &'static str {
        match self {
            Channel::Website => tr("Website"),
            Channel::GitHub => "GitHub",
            Channel::Discord => "Discord",
            Channel::Donate => tr("Donate"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Channel::Website => Icon::Globe,
            Channel::GitHub => Icon::Code,
            Channel::Discord => Icon::People,
            Channel::Donate => Icon::HeartBubble,
        }
    }

    /// Where it leads: a title, what is there and its address for each of
    /// its cards — BananaV's own places; donations through Ko-fi from any
    /// country and through PIX from Brazil.
    fn places(self) -> Vec<(&'static str, &'static str, &'static str)> {
        match self {
            Channel::Website => vec![(
                tr("BananaV's website"),
                tr("News, downloads and help."),
                "https://bananav-editor.github.io/",
            )],
            Channel::GitHub => vec![(
                tr("BananaV on GitHub"),
                tr(
                    "Each version's downloads and what changed in it, and where to tell us about a problem.",
                ),
                "https://github.com/acasoq-web/BananaV",
            )],
            Channel::Discord => vec![(
                tr("Discord community"),
                tr("Talk with other BananaV users and share your work."),
                "https://discord.gg/vtC7Yasz98",
            )],
            Channel::Donate => vec![
                (
                    tr("Support BananaV on Ko-fi"),
                    tr("BananaV is free. Support it from any country with a donation on Ko-fi."),
                    "https://ko-fi.com/bananav",
                ),
                (
                    tr("Donate with PIX"),
                    tr("For donors in Brazil: a donation through PIX helps BananaV go on."),
                    "https://pixgg.com/Acaso",
                ),
            ],
        }
    }
}

/// The welcome screen's state: open, the page shown and the choices of a
/// new project.
#[derive(Debug)]
pub struct Welcome {
    pub open: bool,
    pub page: Page,
    shape: Shape,
    /// HD, FullHD or UHD.
    size: usize,
    /// One of [`RATES`].
    rate: usize,
    drop_frame: bool,
    channel: Channel,
}

impl Default for Welcome {
    /// Widescreen FullHD at 30p (29.970), as VEGAS proposes.
    fn default() -> Welcome {
        Welcome {
            open: false,
            page: Page::New,
            shape: Shape::Widescreen,
            size: 1,
            rate: 2,
            drop_frame: true,
            channel: Channel::Website,
        }
    }
}

impl Welcome {
    pub fn show(&mut self, ctx: &egui::Context, app: &mut BananaApp) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut action = None;
        crate::floating::Window::new(tr("Welcome to BananaV"))
            .id(egui::Id::new("boas-vindas"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| action = self.ui(ui, app));
        if let Some(a) = action {
            app.request(a);
            open = false;
        }
        self.open = open;
    }

    fn ui(&mut self, ui: &mut egui::Ui, app: &mut BananaApp) -> Option<Pending> {
        let (rect, _) = ui.allocate_exact_size(SIZE, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::panel_dark());
        // The header: the logo in the middle.
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
        paint_logo(&painter, header.center(), 26.0, 20.0, theme::text_strong());
        let body = Rect::from_min_max(
            pos2(rect.left(), header.bottom()),
            pos2(rect.right(), rect.bottom() - FOOTER_H),
        );
        // The side bar.
        let side = Rect::from_min_size(body.min, vec2(SIDE_W, body.height()));
        painter.rect_filled(side, 0.0, theme::bg());
        for (i, page) in Page::ALL.into_iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(side.left() + 4.0, side.top() + 4.0 + i as f32 * 80.0),
                vec2(SIDE_W - 8.0, 76.0),
            );
            if tile(ui, r, page.icon(), page.name(), self.page == page, 30.0).clicked() {
                self.page = page;
            }
        }
        let content = Rect::from_min_max(pos2(side.right() + 6.0, body.top()), body.max);
        let action = match self.page {
            Page::New => self.new_page(ui, content),
            Page::Open => open_page(ui, app, content),
            Page::GettingStarted => {
                self.getting_started(ui, content);
                None
            }
            Page::Language => {
                language_page(ui, app, content);
                None
            }
        };
        // "Show Welcome Screen on Startup".
        let footer = Rect::from_min_max(pos2(rect.left() + 8.0, body.bottom()), rect.max);
        ui.scope_builder(UiBuilder::new().max_rect(footer), |ui| {
            ui.add_space(6.0);
            let mut on = app.settings.show_welcome;
            if crate::widgets::checkbox(ui, &mut on, tr("Show Welcome Screen on Startup")).changed()
            {
                app.settings.show_welcome = on;
                let _ = app.settings.save();
            }
        });
        action
    }

    /// New Project: the banner, the five shapes and the new project's
    /// size and rate.
    fn new_page(&mut self, ui: &mut egui::Ui, content: Rect) -> Option<Pending> {
        let painter = ui.painter_at(content);
        let banner = Rect::from_min_size(content.min, vec2(content.width(), BANNER_H));
        paint_banner(&painter.with_clip_rect(banner), banner);
        // The shapes, side by side.
        let row = Rect::from_min_size(
            pos2(content.left(), banner.bottom() + 4.0),
            vec2(content.width(), SHAPE_H),
        );
        let w = row.width() / Shape::ALL.len() as f32;
        for (i, shape) in Shape::ALL.into_iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(row.left() + i as f32 * w, row.top()),
                vec2(w - 4.0, row.height()),
            );
            if tile(ui, r, shape.icon(), shape.name(), self.shape == shape, 28.0).clicked() {
                self.shape = shape;
            }
        }
        let form = Rect::from_min_max(pos2(content.left(), row.bottom() + 4.0), content.max);
        painter.rect_filled(form, 0.0, theme::panel());
        let mut action = None;
        ui.scope_builder(
            UiBuilder::new().max_rect(form.shrink2(vec2(12.0, 8.0))),
            |ui| {
                ui.label(RichText::new(tr("Create a new project")).strong());
                ui.add_space(4.0);
                let sizes = self.shape.sizes();
                // The labels' column as wide as the longer of them.
                let font = egui::TextStyle::Body.resolve(ui.style());
                let label_w = [tr("Resolution:"), tr("Framerate:")]
                    .iter()
                    .map(|t| {
                        ui.painter()
                            .layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE)
                            .size()
                            .x
                    })
                    .fold(0.0, f32::max)
                    + 10.0;
                let label = |ui: &mut egui::Ui, text: &str| {
                    ui.allocate_ui_with_layout(
                        vec2(label_w, 20.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.set_min_width(label_w);
                            ui.label(text)
                        },
                    );
                };
                let full_w = ui.available_width() - label_w;
                ui.horizontal(|ui| {
                    label(ui, tr("Resolution:"));
                    let (w, h) = sizes[self.size];
                    egui::ComboBox::from_id_salt("boas-vindas-resolucao")
                        .width(full_w - 8.0)
                        .selected_text(format!("{} ({w} x {h})", SIZES[self.size]))
                        .show_ui(ui, |ui| {
                            for (i, (w, h)) in sizes.into_iter().enumerate() {
                                let text = format!("{} ({w} x {h})", SIZES[i]);
                                ui.selectable_value(&mut self.size, i, text);
                            }
                        });
                });
                ui.horizontal(|ui| {
                    label(ui, tr("Framerate:"));
                    let drop = self.drop_frame;
                    egui::ComboBox::from_id_salt("boas-vindas-taxa")
                        .width(full_w * 0.66 - 8.0)
                        .selected_text(rate_label(RATES[self.rate], drop))
                        .show_ui(ui, |ui| {
                            for (i, n) in RATES.into_iter().enumerate() {
                                ui.selectable_value(&mut self.rate, i, rate_label(n, drop));
                            }
                        });
                    let pull = |on: bool| {
                        if on {
                            tr("Drop Frame (1/1.001)")
                        } else {
                            tr("Full Frame")
                        }
                    };
                    egui::ComboBox::from_id_salt("boas-vindas-drop-frame")
                        .width(ui.available_width() - 2.0)
                        .selected_text(pull(self.drop_frame))
                        .show_ui(ui, |ui| {
                            for on in [false, true] {
                                ui.selectable_value(&mut self.drop_frame, on, pull(on));
                            }
                        });
                });
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(label_w);
                    let (width, height) = sizes[self.size];
                    let rate = frame_rate(RATES[self.rate], self.drop_frame);
                    let make = |advanced| Pending::NewSized {
                        width,
                        height,
                        rate,
                        advanced,
                    };
                    if ui
                        .add_sized([128.0, 22.0], egui::Button::new(tr("Create Project")))
                        .clicked()
                    {
                        action = Some(make(false));
                    }
                    if ui
                        .add_sized([128.0, 22.0], egui::Button::new(tr("Advanced Settings…")))
                        .on_hover_text(tr(
                            "A new project with these settings, in Project Properties",
                        ))
                        .clicked()
                    {
                        action = Some(make(true));
                    }
                });
            },
        );
        action
    }

    /// Getting Started: the place of the chosen tab over BananaV's own
    /// backdrop, the tabs under it.
    fn getting_started(&mut self, ui: &mut egui::Ui, content: Rect) {
        let painter = ui.painter_at(content);
        let tabs_h = 76.0;
        let art = Rect::from_min_max(
            content.min,
            pos2(content.right(), content.bottom() - tabs_h),
        );
        paint_backdrop(&painter.with_clip_rect(art), art);
        // The place's cards, side by side: a click opens one in the browser.
        let places = self.channel.places();
        let size = vec2(330.0, 210.0);
        let gap = 16.0;
        let n = places.len() as f32;
        let left = art.center().x - (n * size.x + (n - 1.0) * gap) / 2.0;
        for (i, (title, about, url)) in places.into_iter().enumerate() {
            let card = Rect::from_min_size(
                pos2(
                    left + i as f32 * (size.x + gap),
                    art.center().y - size.y / 2.0,
                ),
                size,
            );
            place_card(ui, &painter, card, self.channel.icon(), [title, about, url]);
        }
        // The tabs.
        let tabs = Rect::from_min_max(pos2(content.left(), art.bottom()), content.max);
        painter.rect_filled(tabs, 0.0, theme::panel());
        let w = tabs.width() / Channel::ALL.len() as f32;
        for (i, channel) in Channel::ALL.into_iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(tabs.left() + i as f32 * w + 2.0, tabs.top() + 4.0),
                vec2(w - 4.0, tabs_h - 8.0),
            );
            if tile(
                ui,
                r,
                channel.icon(),
                channel.name(),
                self.channel == channel,
                26.0,
            )
            .clicked()
            {
                self.channel = channel;
            }
        }
    }
}

/// A Getting Started card: its picture with the icon, its title, what is
/// there and its address, shortened; a click opens the address in the
/// browser.
fn place_card(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    card: Rect,
    icon: Icon,
    [title, about, url]: [&str; 3],
) {
    let resp = ui.interact(
        card,
        ui.id().with(("boas-vindas-lugar", title)),
        Sense::click(),
    );
    let lit = resp.hovered();
    if lit {
        crate::cursors::system(ui.ctx(), egui::CursorIcon::PointingHand);
    }
    painter.rect_filled(
        card,
        6.0,
        if lit {
            Color32::from_rgba_unmultiplied(0x30, 0x30, 0x34, 235)
        } else {
            Color32::from_rgba_unmultiplied(0x1C, 0x1C, 0x20, 225)
        },
    );
    let picture = Rect::from_min_size(card.min, vec2(card.width(), 112.0));
    gradient(
        painter,
        picture,
        [
            Color32::from_rgb(0x2A, 0x33, 0x5C),
            Color32::from_rgb(0x5A, 0x2E, 0x6E),
            Color32::from_rgb(0x8A, 0x4A, 0x3A),
            Color32::from_rgb(0x2E, 0x26, 0x4A),
        ],
    );
    icons::paint(
        painter,
        Rect::from_center_size(picture.center(), vec2(54.0, 54.0)),
        icon,
        Color32::WHITE,
    );
    let text_x = card.left() + 14.0;
    painter.text(
        pos2(text_x, picture.bottom() + 12.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(15.0),
        theme::text_strong(),
    );
    let wrap = card.width() - 28.0;
    let galley = painter.layout(
        about.to_string(),
        FontId::proportional(12.0),
        theme::text_dim(),
        wrap,
    );
    painter.galley(
        pos2(text_x, picture.bottom() + 34.0),
        galley,
        theme::text_dim(),
    );
    let shown = url.trim_start_matches("https://").trim_end_matches('/');
    painter.text(
        pos2(text_x, card.bottom() - 12.0),
        Align2::LEFT_BOTTOM,
        shown,
        FontId::proportional(12.0),
        theme::banana(),
    );
    if resp.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

/// Open Project: Browse… and the recent projects (the last one first).
fn open_page(ui: &mut egui::Ui, app: &BananaApp, content: Rect) -> Option<Pending> {
    ui.painter_at(content)
        .rect_filled(content, 0.0, theme::panel());
    let mut action = None;
    ui.scope_builder(
        UiBuilder::new().max_rect(content.shrink2(vec2(14.0, 10.0))),
        |ui| {
            ui.label(RichText::new(tr("Open existing project")).strong());
            if ui
                .add_sized([128.0, 22.0], egui::Button::new(tr("Browse…")))
                .clicked()
            {
                action = Some(Pending::Open);
            }
            ui.add_space(10.0);
            let recent = &app.settings.recent;
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Open a recent project:")).strong());
                ui.add_space(12.0);
                match recent.first() {
                    Some(path) => {
                        if link(ui, tr("Last opened"), path).clicked() {
                            action = Some(Pending::OpenPath(path.clone()));
                        }
                    }
                    None => {
                        ui.label(RichText::new(tr("Last opened")).color(theme::text_disabled()));
                    }
                }
            });
            // The others under it, by name.
            ui.add_space(4.0);
            for path in recent.iter().skip(1) {
                let name = path.file_stem().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().to_string(),
                );
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    if link(ui, &name, path).clicked() {
                        action = Some(Pending::OpenPath(path.clone()));
                    }
                });
            }
        },
    );
    action
}

/// Language: the interface's languages by their own names; the one picked
/// is used at once (Options › Preferences › Language changes it later).
fn language_page(ui: &mut egui::Ui, app: &mut BananaApp, content: Rect) {
    ui.painter_at(content)
        .rect_filled(content, 0.0, theme::panel());
    ui.scope_builder(
        UiBuilder::new().max_rect(content.shrink2(vec2(14.0, 10.0))),
        |ui| {
            ui.label(RichText::new(tr("Language")).strong().size(15.0));
            ui.add_space(2.0);
            ui.label(
                RichText::new(tr(
                    "Choose the language of BananaV's interface. You can change it later in Options › Preferences › Language.",
                ))
                .color(theme::text_dim()),
            );
            ui.add_space(10.0);
            let current = crate::i18n::language();
            // Two columns, scrolled within the page (twenty languages are
            // taller than it).
            egui::ScrollArea::vertical()
                .id_salt("welcome-languages")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for pair in Language::ALL.chunks(2) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            for &lang in pair {
                                language_tile(ui, app, lang, lang == current);
                            }
                        });
                        ui.add_space(6.0);
                    }
                });
        },
    );
}

/// One language on the Language page, by its own name (partial ones say
/// so under it): picked at once when clicked.
fn language_tile(ui: &mut egui::Ui, app: &mut BananaApp, lang: Language, on: bool) {
    let (r, resp) = ui.allocate_exact_size(vec2(300.0, 46.0), Sense::click());
    let fill = if on {
        theme::selection()
    } else if resp.hovered() {
        theme::hover()
    } else {
        theme::panel_dark()
    };
    ui.painter().rect_filled(r, 4.0, fill);
    let color = if on { Color32::WHITE } else { theme::text() };
    if lang.partial() {
        ui.painter().text(
            pos2(r.left() + 16.0, r.top() + 15.0),
            Align2::LEFT_CENTER,
            lang.native_name(),
            FontId::proportional(15.0),
            color,
        );
        ui.painter().text(
            pos2(r.left() + 16.0, r.bottom() - 12.0),
            Align2::LEFT_CENTER,
            tr("(partial translation)"),
            FontId::proportional(11.0),
            if on {
                Color32::WHITE
            } else {
                theme::text_dim()
            },
        );
    } else {
        ui.painter().text(
            pos2(r.left() + 16.0, r.center().y),
            Align2::LEFT_CENTER,
            lang.native_name(),
            FontId::proportional(16.0),
            color,
        );
    }
    if on {
        // A check mark at its right end.
        let c = pos2(r.right() - 22.0, r.center().y);
        ui.painter().add(egui::Shape::line(
            vec![
                c + vec2(-6.0, 0.0),
                c + vec2(-2.0, 4.0),
                c + vec2(6.0, -5.0),
            ],
            Stroke::new(2.0, Color32::WHITE),
        ));
    }
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, on, lang.native_name())
    });
    if resp.clicked() && !on {
        app.set_language(lang);
        let _ = app.settings.save();
    }
}

/// A link (underlined, as VEGAS's), the file's path in its tip.
fn link(ui: &mut egui::Ui, text: &str, path: &std::path::Path) -> egui::Response {
    let resp = ui.add(
        egui::Label::new(
            RichText::new(text)
                .underline()
                .strong()
                .color(theme::text_strong()),
        )
        .sense(Sense::click()),
    );
    if resp.hovered() {
        crate::cursors::system(ui.ctx(), egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(path.display().to_string())
}

/// A button of the side bar, the shapes or Getting Started's tabs: its
/// icon over its name, blue when chosen.
fn tile(
    ui: &mut egui::Ui,
    r: Rect,
    icon: Icon,
    name: &str,
    on: bool,
    icon_size: f32,
) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(("boas-vindas-botao", name)), Sense::click());
    let painter = ui.painter();
    if on {
        painter.rect_filled(r, 0.0, theme::selection());
    } else if resp.hovered() {
        painter.rect_filled(r, 0.0, theme::hover());
    }
    let color = if on { Color32::WHITE } else { theme::text() };
    let icon_rect = Rect::from_center_size(
        pos2(r.center().x, r.top() + 8.0 + icon_size / 2.0),
        vec2(icon_size, icon_size),
    );
    icons::paint(painter, icon_rect, icon, color);
    let galley = painter.layout(
        name.to_string(),
        FontId::proportional(12.0),
        color,
        r.width() - 6.0,
    );
    let at = pos2(
        r.center().x - galley.size().x / 2.0,
        (icon_rect.bottom() + 6.0).min(r.bottom() - galley.size().y - 4.0),
    );
    painter.galley(at, galley, color);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name));
    resp
}

/// BananaV's logo and its name, centered on `center`.
fn paint_logo(painter: &egui::Painter, center: Pos2, icon: f32, text: f32, color: Color32) {
    let galley = painter.layout_no_wrap("BananaV".to_string(), FontId::proportional(text), color);
    let width = icon + 8.0 + galley.size().x;
    let left = center.x - width / 2.0;
    icons::paint_logo(
        painter,
        Rect::from_center_size(pos2(left + icon / 2.0, center.y), vec2(icon, icon)),
    );
    painter.galley(
        pos2(left + icon + 8.0, center.y - galley.size().y / 2.0),
        galley,
        color,
    );
}

/// A rectangle filled with a gradient between its corners' colors (top
/// left, top right, bottom right, bottom left).
fn gradient(painter: &egui::Painter, r: Rect, corners: [Color32; 4]) {
    let mut mesh = egui::Mesh::default();
    for (p, c) in [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
    ]
    .into_iter()
    .zip(corners)
    {
        mesh.colored_vertex(p, c);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// A soft round light of `color` at `center`.
fn glow(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32, strength: f32) {
    let steps = 14;
    for i in 0..steps {
        let t = 1.0 - i as f32 / steps as f32;
        let a = (strength * 255.0 / steps as f32) as u8;
        painter.circle_filled(
            center,
            radius * t,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a),
        );
    }
}

/// The New Project banner, BananaV's own: a dusk sky, warm lights, a
/// timeline's events rising to the right and the logo.
fn paint_banner(painter: &egui::Painter, r: Rect) {
    gradient(
        painter,
        r,
        [
            Color32::from_rgb(0x1B, 0x22, 0x46),
            Color32::from_rgb(0x4A, 0x24, 0x62),
            Color32::from_rgb(0x9A, 0x3D, 0x3A),
            Color32::from_rgb(0x22, 0x1F, 0x3E),
        ],
    );
    glow(
        painter,
        r.right_bottom() + vec2(-120.0, -10.0),
        240.0,
        Color32::from_rgb(0xFF, 0xB0, 0x40),
        0.35,
    );
    glow(
        painter,
        r.left_top() + vec2(150.0, 70.0),
        170.0,
        Color32::from_rgb(0x50, 0x80, 0xFF),
        0.25,
    );
    // A timeline's events: three tracks of colored blocks and the cursor.
    let colors = [
        Color32::from_rgba_unmultiplied(0x4C, 0x8D, 0xE0, 200),
        Color32::from_rgba_unmultiplied(0x3F, 0xB4, 0xC8, 200),
        Color32::from_rgba_unmultiplied(0x5E, 0xC2, 0x6A, 200),
    ];
    let lane_h = 22.0;
    let base = pos2(r.left() + 400.0, r.bottom() - 28.0 - 3.0 * (lane_h + 8.0));
    let blocks: [[(f32, f32); 3]; 3] = [
        [(0.0, 90.0), (96.0, 70.0), (172.0, 120.0)],
        [(30.0, 110.0), (146.0, 60.0), (212.0, 80.0)],
        [(0.0, 150.0), (156.0, 140.0), (300.0, 0.0)],
    ];
    for (row, (lane, color)) in blocks.iter().zip(colors).enumerate() {
        let y = base.y + row as f32 * (lane_h + 8.0);
        for &(x, w) in lane {
            if w > 0.0 {
                let b = Rect::from_min_size(pos2(base.x + x, y), vec2(w, lane_h));
                painter.rect_filled(b, 4.0, color);
            }
        }
    }
    let cursor_x = base.x + 140.0;
    painter.line_segment(
        [
            pos2(cursor_x, base.y - 12.0),
            pos2(cursor_x, base.y + 3.0 * (lane_h + 8.0)),
        ],
        Stroke::new(2.0, Color32::from_white_alpha(220)),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            pos2(cursor_x - 6.0, base.y - 20.0),
            pos2(cursor_x + 6.0, base.y - 20.0),
            pos2(cursor_x + 6.0, base.y - 14.0),
            pos2(cursor_x, base.y - 9.0),
            pos2(cursor_x - 6.0, base.y - 14.0),
        ],
        Color32::from_white_alpha(230),
        Stroke::NONE,
    ));
    // The logo and what it is.
    let logo = pos2(r.left() + 46.0, r.center().y - 22.0);
    icons::paint_logo(
        painter,
        Rect::from_min_size(logo - vec2(0.0, 40.0), vec2(84.0, 84.0)),
    );
    painter.text(
        logo + vec2(98.0, -6.0),
        Align2::LEFT_CENTER,
        "BananaV",
        FontId::proportional(44.0),
        Color32::WHITE,
    );
    painter.text(
        logo + vec2(100.0, 30.0),
        Align2::LEFT_CENTER,
        tr("Video editor"),
        FontId::proportional(16.0),
        Color32::from_white_alpha(210),
    );
}

/// Getting Started's backdrop, BananaV's own: a dark sky with warm and
/// cold lights.
fn paint_backdrop(painter: &egui::Painter, r: Rect) {
    gradient(
        painter,
        r,
        [
            Color32::from_rgb(0x1A, 0x1C, 0x30),
            Color32::from_rgb(0x3A, 0x1F, 0x48),
            Color32::from_rgb(0x5A, 0x2A, 0x2E),
            Color32::from_rgb(0x16, 0x18, 0x28),
        ],
    );
    glow(
        painter,
        r.right_top() + vec2(-140.0, 90.0),
        220.0,
        Color32::from_rgb(0xFF, 0x7A, 0x50),
        0.3,
    );
    glow(
        painter,
        r.left_bottom() + vec2(130.0, -70.0),
        200.0,
        Color32::from_rgb(0x50, 0x70, 0xFF),
        0.25,
    );
}

impl BananaApp {
    /// The welcome screen open on `page` ("closed": not shown, whatever
    /// the user's settings say), for screen tests.
    pub fn welcome_for_review(&mut self, page: &str) {
        let w = &mut self.menu_dialogs.welcome;
        w.open = page != "closed";
        w.page = match page {
            "open" => Page::Open,
            "getting started" => Page::GettingStarted,
            "language" => Page::Language,
            _ => Page::New,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shapes_offer_vegas_sizes_and_rates() {
        assert_eq!(Shape::Widescreen.sizes()[1], (1920, 1080));
        assert_eq!(Shape::Scope.sizes()[2], (4096, 1716));
        assert_eq!(Shape::Portrait.sizes()[0], (720, 1280));
        assert_eq!(frame_rate(30, true), FrameRate::FPS_29_97);
        assert_eq!(frame_rate(25, false), FrameRate::FPS_25);
        crate::i18n::set_language(Language::English);
        assert_eq!(rate_label(30, true), "30p (29.970)");
        assert_eq!(rate_label(25, true), "25p (24.975)");
        assert_eq!(rate_label(120, false), "120p (120.000)");
        // VEGAS's: Widescreen FullHD at 30p (29.970).
        let w = Welcome::default();
        assert_eq!(
            (w.shape, w.size, RATES[w.rate], w.drop_frame),
            (Shape::Widescreen, 1, 30, true)
        );
    }
}
