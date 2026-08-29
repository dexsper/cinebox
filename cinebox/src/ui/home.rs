use std::collections::HashMap;

use cinebox_core::i18n::Msg;
use cinebox_core::{CatalogItem, HomeCatalog, HomeRow, MediaKind, TmdbId, typograph};
use iced::advanced::Renderer as _;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::widget::image::Handle as ImageHandle;
use iced::widget::text::Wrapping;
use iced::widget::{Row, button, column, container, mouse_area, stack, text};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Fill, Length, Rectangle, Shadow,
    Size,
};

use crate::app::Message;
use crate::ui::scroll::{self, ScrollFlash, ScrollPane};

const MUTED: Color = Color::from_rgb(0.65, 0.65, 0.68);
const ERR: Color = Color::from_rgb(0.92, 0.38, 0.38);
const TILE_WIDTH: f32 = 140.0;
const POSTER_HEIGHT: f32 = 210.0;
pub const POSTER_RADIUS: f32 = 12.0;
const RING_WIDTH: f32 = 3.0;
const RING_GAP: f32 = 4.0;
const RING_PAD: f32 = RING_WIDTH + RING_GAP;
const RING: Color = Color::from_rgb(0.94, 0.94, 0.96);
const TITLE_SLACK: f32 = 72.0;

pub fn tile_metrics() -> (f32, f32) {
    (TILE_WIDTH, POSTER_HEIGHT)
}

pub fn catalog_shelf_height() -> f32 {
    let (_, poster_h) = tile_metrics();
    poster_h + RING_PAD * 2.0 + TITLE_SLACK
}

/// Shared catalog tile (home rows and card shelves).
pub fn catalog_tile<'a>(
    item: &'a CatalogItem,
    poster: Option<&'a ImageHandle>,
) -> Element<'a, Message> {
    tile(item, poster)
}

pub type PosterMap = HashMap<(MediaKind, TmdbId), ImageHandle>;
pub type ExtraImages = HashMap<String, ImageHandle>;

pub enum HomeState {
    NeedKey,
    Loading,
    Ready(HomeCatalog),
    Failed(String),
}

pub fn view<'a>(
    state: &'a HomeState,
    posters: &'a PosterMap,
    scroll: &'a ScrollFlash,
) -> Element<'a, Message> {
    match state {
        HomeState::NeedKey => need_key(),
        HomeState::Loading => centered_status(Msg::LoadingHome.en()),
        HomeState::Failed(error) => failed(error),
        HomeState::Ready(catalog) => catalog_view(catalog, posters, scroll),
    }
}

fn need_key() -> Element<'static, Message> {
    container(
        column![
            text(Msg::HomeTitle.en()).size(20),
            text(Msg::NeedTmdbKey.en()).size(14).color(MUTED),
            button(text(Msg::NavSettings.en())).on_press(Message::OpenSettings),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn centered_status(message: &'static str) -> Element<'static, Message> {
    container(
        column![
            text(Msg::HomeTitle.en()).size(20),
            text(message).color(MUTED)
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn failed(error: &str) -> Element<'static, Message> {
    container(
        column![
            text(Msg::HomeTitle.en()).size(20),
            text(error.to_owned()).size(14).color(ERR),
            button(text("Retry")).on_press(Message::RetryHome),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Fill)
    .height(Fill)
    .into()
}

fn catalog_view<'a>(
    catalog: &'a HomeCatalog,
    posters: &'a PosterMap,
    scroll: &'a ScrollFlash,
) -> Element<'a, Message> {
    let mut body = column![text(Msg::HomeTitle.en()).size(20)].spacing(20);
    for (index, row) in catalog.rows.iter().enumerate() {
        let row_index = u8::try_from(index).unwrap_or(u8::MAX);
        body = body.push(shelf(row, posters, row_index, scroll.row(row_index)));
    }
    container(scroll::vertical(scroll.page(), body.padding([0, 8])))
        .padding(16)
        .width(Fill)
        .height(Fill)
        .into()
}

fn shelf<'a>(
    row: &'a HomeRow,
    posters: &'a PosterMap,
    index: u8,
    flashing: bool,
) -> Element<'a, Message> {
    let mut block = column![text(row.id.title()).size(16)].spacing(8);
    if let Some(error) = &row.error {
        block = block.push(text(error.as_str()).size(13).color(ERR));
    }
    if row.items.is_empty() {
        if row.error.is_none() {
            block = block.push(text(Msg::EmptyRow.en()).size(13).color(MUTED));
        }
        return block.into();
    }
    let tiles = Row::with_children(
        row.items
            .iter()
            .map(|item| tile(item, posters.get(&(item.kind, item.id)))),
    );
    block = block.push(scroll::horizontal(
        ScrollPane::Row(index),
        flashing,
        catalog_shelf_height(),
        tiles.spacing(12).padding(8),
    ));
    block.into()
}

fn tile<'a>(item: &'a CatalogItem, poster: Option<&'a ImageHandle>) -> Element<'a, Message> {
    let year = item
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| String::from("—"));
    let well_w = TILE_WIDTH + RING_PAD * 2.0;
    let body = column![
        container(poster_block(poster, item.vote)).padding(RING_PAD),
        text(typograph(&item.title))
            .size(13)
            .width(TILE_WIDTH)
            .wrapping(Wrapping::Word),
        text(year)
            .size(12)
            .color(MUTED)
            .width(TILE_WIDTH)
            .align_x(Alignment::Start),
    ]
    .spacing(4)
    .width(well_w)
    .align_x(Alignment::Center);
    hover_ring(
        mouse_area(body)
            .on_release(Message::OpenMedia {
                kind: item.kind,
                id: item.id,
            })
            .into(),
    )
}

fn poster_block<'a>(poster: Option<&'a ImageHandle>, vote: Option<f32>) -> Element<'a, Message> {
    let art: Element<'a, Message> = match poster {
        Some(handle) => iced::widget::image(handle)
            .width(TILE_WIDTH)
            .height(POSTER_HEIGHT)
            .content_fit(ContentFit::Cover)
            .border_radius(POSTER_RADIUS)
            .into(),
        None => container(text(" ").size(1))
            .width(TILE_WIDTH)
            .height(POSTER_HEIGHT)
            .style(|_| container::Style {
                background: Some(Color::from_rgb(0.16, 0.16, 0.18).into()),
                border: iced::border::rounded(POSTER_RADIUS),
                ..container::Style::default()
            })
            .into(),
    };

    let mut layers = vec![art];
    if let Some(vote) = vote {
        layers.push(
            container(vote_badge(vote))
                .width(Fill)
                .height(Fill)
                .padding(6)
                .align_x(Alignment::End)
                .align_y(Alignment::End)
                .into(),
        );
    }
    stack(layers)
        .width(TILE_WIDTH)
        .height(POSTER_HEIGHT)
        .clip(true)
        .into()
}

fn vote_badge<'a>(vote: f32) -> Element<'a, Message> {
    container(text(format!("{vote:.1}")).size(12))
        .padding([2, 6])
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.75).into()),
            text_color: Some(Color::from_rgb(1.0, 0.85, 0.25)),
            border: iced::border::rounded(4),
            ..container::Style::default()
        })
        .into()
}

fn hover_ring<'a>(content: Element<'a, Message>) -> Element<'a, Message> {
    Hover { content }.into()
}

struct Hover<'a> {
    content: Element<'a, Message>,
}

struct HoverState {
    hovered: bool,
}

impl Widget<Message, iced::Theme, iced::Renderer> for Hover<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<HoverState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(HoverState { hovered: false })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let over = cursor.is_over(layout.bounds());
        let state = tree.state.downcast_mut::<HoverState>();
        if state.hovered != over {
            state.hovered = over;
            shell.request_redraw();
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if !tree.state.downcast_ref::<HoverState>().hovered {
            return;
        }
        let bounds = layout.bounds();
        let ring = Rectangle {
            x: bounds.x,
            y: bounds.y,
            width: TILE_WIDTH + RING_PAD * 2.0,
            height: POSTER_HEIGHT + RING_PAD * 2.0,
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: ring,
                border: Border {
                    color: RING,
                    width: RING_WIDTH,
                    radius: (POSTER_RADIUS + RING_PAD).into(),
                },
                shadow: Shadow::default(),
                snap: true,
            },
            Background::Color(Color::TRANSPARENT),
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Hover<'a>> for Element<'a, Message> {
    fn from(hover: Hover<'a>) -> Self {
        Self::new(hover)
    }
}
