use rusty_core::rename::check::{Existing, ExistingName, Fold};
use rusty_core::rename::{FieldRegistry, RenameRules};
use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};
use rusty_ui::multi_rename::view::fields::{
    FieldInputs, FieldMenu, Fields, Mask, MenuRow, MenuSlot, menu_tokens,
};
use rusty_ui::multi_rename::view::{TextareaState, name_editor, name_tools};
use rusty_ui::multi_rename::{
    Form, Item, Listing, Preview, PreviewView, RenamePanel, Shown, rename_title, scrim,
};
use rusty_ui::prelude::*;
use rusty_ui::primitives::{TextFieldState, text_field_state};

use crate::window_page::WindowPage;

const FOLDER: &str = "/work/photos/2026-10 Iceland";
const PHOTOS: usize = 40;

fn input(value: &str, window: &mut Window, cx: &mut App) -> Entity<TextFieldState> {
    let state = text_field_state("", window, cx);
    state.update(cx, |state, cx| {
        state.set_value(value.to_owned(), window, cx)
    });
    state
}

fn inputs(rules: &RenameRules, window: &mut Window, cx: &mut App) -> FieldInputs {
    let counter = rules.counter;
    FieldInputs {
        name_mask: input(&rules.name_mask, window, cx),
        ext_mask: input(&rules.ext_mask, window, cx),
        find: input(&rules.find.text, window, cx),
        replace: input(&rules.find.replace, window, cx),
        start: input(&counter.start.to_string(), window, cx),
        step: input(&counter.step.to_string(), window, cx),
        digits: input(&counter.digits.to_string(), window, cx),
    }
}

fn photo(folder: &Location, name: &str) -> Item {
    let entry = FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 0,
        modified_ms: Some(1_792_000_000_000),
        attributes: Attributes::default(),
    };
    Item::from_entry(folder, &entry)
}

fn sample() -> Option<(Form, Vec<Item>, Preview)> {
    let folder = Location::new(FOLDER).ok()?;
    let mut names: Vec<String> = (1..=PHOTOS).map(|n| format!("IMG_{n:04}.JPG")).collect();
    names.push("notes.txt".to_owned());
    let items: Vec<Item> = names.iter().map(|name| photo(&folder, name)).collect();
    let mut form = Form::new();
    form.rules.find.text = "IMG".to_owned();
    form.rules.find.replace = "Iceland".to_owned();
    form.rules.ext_case = rusty_core::rename::ExtCase::Lower;
    form.toggle_excluded(PHOTOS);
    let mut marked = items.clone();
    form.mark_excluded(&mut marked);
    let existing = Existing::new(
        names
            .iter()
            .cloned()
            .chain(["Iceland_0007.jpg".to_owned()])
            .map(|name| ExistingName {
                parent: folder.clone(),
                name,
            })
            .collect(),
    );
    let listing = Listing {
        existing: &existing,
        fold: Fold::Exact,
    };
    let preview = Preview::compute(&form.compile().ok()?, &marked, listing, 1);
    Some((form, marked, preview))
}

pub struct MultiRenameToolPage {
    window: Entity<WindowPage>,
    inputs: FieldInputs,
    filter: Entity<TextFieldState>,
    view: Entity<PreviewView>,
    rules: RenameRules,
    count: usize,
    has_menu: bool,
    names: Entity<TextareaState>,
}

impl MultiRenameToolPage {
    fn new(has_menu: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (form, items, preview) = sample().unwrap_or_else(|| {
            let form = Form::new();
            let preview = Preview {
                generation: 0,
                rows: Vec::new(),
                summary: rusty_ui::multi_rename::Summary::default(),
                plan: None,
            };
            (form, Vec::new(), preview)
        });
        let view = cx.new(|cx| PreviewView::new(window, cx));
        let compiled = form.compile().ok();
        let text = preview
            .rows
            .iter()
            .map(|row| row.new.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let names = cx.new(|cx| TextareaState::new(window, cx).default_value(text));
        view.update(cx, |view, cx| {
            view.set_preview(
                Shown {
                    items: &items,
                    preview: &preview,
                    fold: Fold::Exact,
                    problems_only: false,
                    matcher: compiled.as_ref(),
                    edited: false,
                },
                cx,
            );
            view.set_cursor(Some(6), cx);
        });
        Self {
            window: cx.new(|cx| WindowPage::new(None, cx)),
            inputs: inputs(&form.rules, window, cx),
            filter: input("date", window, cx),
            view,
            rules: form.rules,
            count: items.len(),
            has_menu,
            names,
        }
    }

    fn menu(&self) -> FieldMenu {
        let rows = menu_tokens("date", &FieldRegistry::new())
            .into_iter()
            .map(|token| MenuRow {
                text: token.text.into(),
                is_current: false,
            })
            .collect();
        FieldMenu::new("tool-menu", rows)
            .filter(&self.filter)
            .selected(0)
    }
}

impl Render for MultiRenameToolPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut fields = Fields::new("tool-fields", &self.inputs, &self.rules);
        if self.has_menu {
            fields = fields.menu(MenuSlot::Mask(Mask::Name), self.menu());
        }
        let title = rename_title(self.count, "2026-10 Iceland");
        let panel = RenamePanel::new(
            title,
            fields,
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .when(!self.has_menu, |area| {
                    area.child(name_editor(&self.names, None, cx))
                })
                .child(self.view.clone()),
        )
        .tools(name_tools(false, cx));
        div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .child(self.window.clone())
            .child(scrim().child(panel))
    }
}

pub type ToolPages = [Entity<MultiRenameToolPage>; 2];

pub fn tool_pages(window: &mut Window, cx: &mut App) -> ToolPages {
    [false, true].map(|has_menu| cx.new(|cx| MultiRenameToolPage::new(has_menu, window, cx)))
}
