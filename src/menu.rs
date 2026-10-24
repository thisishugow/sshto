use crate::hosts::Host;
use unicode_width::UnicodeWidthChar;

pub fn cell_width(text: &str) -> usize {
    // Bash measures individual code points, not grapheme clusters.
    text.chars().map(|ch| ch.width().unwrap_or(0)).sum()
}

pub fn truncate_cells(text: &str, width: usize) -> String {
    let mut cells = 0;
    text.chars()
        .take_while(|ch| {
            cells += ch.width().unwrap_or(0);
            cells <= width
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Tab {
    pub label: String,
    pub count: usize,
    pub text: String,
    pub width: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayRow {
    Heading(usize),
    Host(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabHitbox {
    pub tab: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Header {
    Hidden,
    Tabs,
    Compact(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub rows: usize,
    pub cols: usize,
    pub width: usize,
    pub header_rows: usize,
    pub footer_rows: usize,
    pub slots: usize,
    pub offset: usize,
    pub selected_row: usize,
    pub header: Header,
    pub controls: Option<String>,
    pub tab_hitboxes: Vec<TabHitbox>,
    pub indent: usize,
    pub alias_width: isize,
    pub info_width: isize,
    pub footer: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Tab,
    ShiftTab,
    Digit(u8),
    Enter,
    Quit,
    Interrupt,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Connect(usize),
    Exit(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MousePhase {
    Press,
    Release,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub button: u16,
    // Coordinates are one-based, as in the SGR reports consumed by Bash.
    pub x: usize,
    pub y: usize,
    pub phase: MousePhase,
}

#[derive(Debug)]
pub struct Menu {
    hosts: Vec<Host>,
    tabs: Vec<Tab>,
    tab: usize,
    cur: usize,
    offset: usize,
    visible: Vec<usize>,
    display_rows: Vec<DisplayRow>,
    host_rows: Vec<usize>,
    last_click: Option<(usize, u64)>,
    mouse_press_y: Option<usize>,
    last_layout: Option<Layout>,
    resized: bool,
}

impl Menu {
    pub fn new(labels: Vec<String>, hosts: Vec<Host>) -> Self {
        let mut counts = vec![0; labels.len()];
        for host in &hosts {
            counts[host.group] += 1;
        }
        let tabs = std::iter::once(("All".to_owned(), hosts.len()))
            .chain(labels.into_iter().zip(counts))
            .enumerate()
            .map(|(index, (label, count))| {
                let text = format!(" {index}:{label}({count}) ");
                let width = cell_width(&text);
                Tab {
                    label,
                    count,
                    text,
                    width,
                }
            })
            .collect();
        let mut menu = Self {
            hosts,
            tabs,
            tab: 0,
            cur: 0,
            offset: 0,
            visible: Vec::new(),
            display_rows: Vec::new(),
            host_rows: Vec::new(),
            last_click: None,
            mouse_press_y: None,
            last_layout: None,
            resized: false,
        };
        menu.select_tab(0);
        menu
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }
    pub fn hosts(&self) -> &[Host] {
        &self.hosts
    }
    pub fn tab(&self) -> usize {
        self.tab
    }
    pub fn selection(&self) -> usize {
        self.cur
    }
    pub fn visible(&self) -> &[usize] {
        &self.visible
    }
    pub fn display_rows(&self) -> &[DisplayRow] {
        &self.display_rows
    }
    pub fn selected_host(&self) -> Option<usize> {
        self.visible.get(self.cur).copied()
    }
    pub fn shortcut_max(&self) -> usize {
        (self.tabs.len() - 1).min(9)
    }

    pub fn select_tab(&mut self, tab: usize) {
        if tab >= self.tabs.len() {
            return;
        }
        self.tab = tab;
        self.last_click = None;
        self.mouse_press_y = None;
        self.cur = 0;
        self.offset = 0;
        self.visible.clear();
        self.display_rows.clear();
        self.host_rows.clear();
        for group in 0..self.tabs.len() - 1 {
            if (tab != 0 && group != tab - 1) || self.tabs[group + 1].count == 0 {
                continue;
            }
            if tab == 0 {
                self.display_rows.push(DisplayRow::Heading(group));
            }
            for (index, host) in self.hosts.iter().enumerate() {
                if host.group == group {
                    self.host_rows.push(self.display_rows.len());
                    self.display_rows.push(DisplayRow::Host(index));
                    self.visible.push(index);
                }
            }
        }
    }

    fn move_up(&mut self) {
        self.cur = self.cur.saturating_sub(1);
    }
    fn move_down(&mut self) {
        if self.cur + 1 < self.visible.len() {
            self.cur += 1;
        }
    }

    pub fn handle_key(&mut self, key: Key) -> Action {
        self.last_click = None;
        self.mouse_press_y = None;
        match key {
            Key::Up => self.move_up(),
            Key::Down => self.move_down(),
            Key::Right | Key::Tab => self.select_tab((self.tab + 1) % self.tabs.len()),
            Key::Left | Key::ShiftTab => {
                self.select_tab((self.tab + self.tabs.len() - 1) % self.tabs.len())
            }
            Key::Digit(digit) if digit <= 9 => self.select_tab(usize::from(digit)),
            Key::Enter => return self.selected_host().map_or(Action::None, Action::Connect),
            Key::Quit => return Action::Exit(0),
            Key::Interrupt => return Action::Exit(130),
            _ => {}
        }
        Action::None
    }

    pub fn mark_resized(&mut self) {
        self.mouse_press_y = None;
        self.resized = true;
    }

    pub fn layout(&mut self, rows: usize, cols: usize) -> Layout {
        let rows = rows.max(1);
        let cols = cols.max(2);
        let width = cols - 1;
        let header_rows = if rows == 1 {
            0
        } else if rows >= 5 {
            2
        } else {
            1
        };
        let footer_rows = usize::from(rows >= 3);
        let slots = rows - header_rows - footer_rows;
        let selected_row = self.host_rows.get(self.cur).copied().unwrap_or(0);
        if selected_row < self.offset {
            self.offset = selected_row;
            if slots > 1
                && self.offset > 0
                && matches!(self.display_rows[self.offset - 1], DisplayRow::Heading(_))
            {
                self.offset -= 1;
            }
        }
        if selected_row >= self.offset + slots {
            self.offset = selected_row - slots + 1;
        }
        self.offset = self
            .offset
            .min(self.display_rows.len().saturating_sub(slots));
        let mut tab_hitboxes = Vec::new();
        let header = if header_rows == 0 {
            Header::Hidden
        } else if width >= self.tabs.iter().map(|tab| tab.width).sum() {
            let mut column = 1;
            for (index, tab) in self.tabs.iter().enumerate() {
                tab_hitboxes.push(TabHitbox {
                    tab: index,
                    start: column,
                    end: column + tab.width - 1,
                });
                column += tab.width;
            }
            Header::Tabs
        } else {
            let text = if cols >= 24 {
                format!(
                    "{}:{} ({})",
                    self.tab,
                    self.tabs[self.tab].label,
                    self.visible.len()
                )
            } else {
                format!("{}: ({})", self.tab, self.visible.len())
            };
            Header::Compact(truncate_cells(&text, width))
        };
        let controls = (header_rows == 2).then(|| {
            let text = if cols >= 78 {
                format!("  Left/Right Tab: group  0-{}: jump  Up/Down: select  Enter: connect  q/Esc: quit", self.shortcut_max())
            } else {
                "< > Tab: group  Up/Down  Enter  q/Esc".to_owned()
            };
            truncate_cells(&text, width)
        });
        let indent = if self.tab == 0 { 2 } else { 0 };
        let max_len = self
            .visible
            .iter()
            .map(|&index| self.hosts[index].alias.chars().count())
            .max()
            .unwrap_or(0);
        let alias_width = (max_len as isize).min((width as isize - 6 - indent as isize) / 2);
        let info_width = width as isize - alias_width - 6 - indent as isize;
        let footer = (footer_rows == 1).then(|| {
            truncate_cells(
                &format!(
                    "sshto  {}/{}  Click: select  Double-click: connect  Wheel: move",
                    if self.visible.is_empty() {
                        0
                    } else {
                        self.cur + 1
                    },
                    self.visible.len()
                ),
                width,
            )
        });
        let layout = Layout {
            rows,
            cols,
            width,
            header_rows,
            footer_rows,
            slots,
            offset: self.offset,
            selected_row,
            header,
            controls,
            tab_hitboxes,
            indent,
            alias_width,
            info_width,
            footer,
        };
        self.resized = false;
        self.last_layout = Some(layout.clone());
        layout
    }

    pub fn handle_mouse(&mut self, event: MouseEvent, timestamp_ms: u64) -> Action {
        if self.resized {
            self.mouse_press_y = None;
            return Action::None;
        }
        let Some(layout) = &self.last_layout else {
            return Action::None;
        };
        let MouseEvent {
            button,
            x,
            y,
            phase,
        } = event;
        if x < 1 || x >= layout.cols || y < 1 || y > layout.rows {
            return Action::None;
        }
        if button == 0 {
            if phase == MousePhase::Press {
                self.mouse_press_y = Some(y);
                return Action::None;
            }
            if self.mouse_press_y != Some(y) {
                self.mouse_press_y = None;
                self.last_click = None;
                return Action::None;
            }
            self.mouse_press_y = None;
        } else if phase == MousePhase::Release {
            return Action::None;
        } else {
            self.mouse_press_y = None;
        }
        if button == 0 && y == 1 && layout.header_rows > 0 {
            if let Some(hitbox) = layout
                .tab_hitboxes
                .iter()
                .find(|hitbox| x >= hitbox.start && x <= hitbox.end)
            {
                let tab = hitbox.tab;
                self.select_tab(tab);
                return Action::None;
            }
        }
        if y <= layout.header_rows || y > layout.header_rows + layout.slots {
            return Action::None;
        }
        match button {
            0 => {
                let row = self.offset + y - layout.header_rows - 1;
                if let Some(index) = self.host_rows.iter().position(|&host_row| host_row == row) {
                    self.cur = index;
                    let host = self.visible[index];
                    let connect = self.last_click.is_some_and(|(last_host, last_ms)| {
                        last_host == host && timestamp_ms.saturating_sub(last_ms) <= 450
                    });
                    self.last_click = Some((host, timestamp_ms));
                    if connect {
                        return Action::Connect(host);
                    }
                } else {
                    self.last_click = None;
                }
            }
            64 => {
                self.last_click = None;
                self.move_up();
            }
            65 => {
                self.last_click = None;
                self.move_down();
            }
            _ => {
                self.last_click = None;
            }
        }
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, hosts};
    use std::path::Path;

    fn fixture() -> Menu {
        let (config, hosts) = hosts::tests::fixture();
        Menu::new(config.labels, hosts)
    }

    fn names(menu: &Menu) -> Vec<&str> {
        menu.visible()
            .iter()
            .map(|&index| menu.hosts()[index].alias.as_str())
            .collect()
    }

    fn mouse(
        menu: &mut Menu,
        button: u16,
        x: usize,
        y: usize,
        phase: MousePhase,
        ms: u64,
    ) -> Action {
        menu.handle_mouse(
            MouseEvent {
                button,
                x,
                y,
                phase,
            },
            ms,
        )
    }

    fn click(menu: &mut Menu, x: usize, y: usize, ms: u64) -> Action {
        assert_eq!(mouse(menu, 0, x, y, MousePhase::Press, ms), Action::None);
        mouse(menu, 0, x, y, MousePhase::Release, ms)
    }

    #[test]
    fn all_order_headings_and_keyboard_selection() {
        let mut menu = fixture();
        assert_eq!(
            names(&menu),
            [
                "cloud-ts",
                "office-ts",
                "work-a",
                "work-b",
                "home-b",
                "loose"
            ]
        );
        assert_eq!(
            menu.display_rows(),
            [
                DisplayRow::Heading(0),
                DisplayRow::Host(0),
                DisplayRow::Heading(1),
                DisplayRow::Host(3),
                DisplayRow::Host(4),
                DisplayRow::Host(5),
                DisplayRow::Heading(2),
                DisplayRow::Host(1),
                DisplayRow::Heading(3),
                DisplayRow::Host(2),
            ]
        );
        menu.handle_key(Key::Up);
        assert_eq!(menu.selection(), 0);
        for (selection, host) in [0, 3, 4, 5, 1, 2].into_iter().enumerate() {
            assert_eq!(menu.selection(), selection);
            assert_eq!(menu.handle_key(Key::Enter), Action::Connect(host));
            menu.handle_key(Key::Down);
        }
        assert_eq!(menu.selection(), 5);
        menu.handle_key(Key::Digit(2));
        assert_eq!(names(&menu), ["office-ts", "work-a", "work-b"]);
        menu.handle_key(Key::Digit(0));
        menu.handle_key(Key::Left);
        assert_eq!(names(&menu), ["loose"]);
        menu.handle_key(Key::Right);
        assert_eq!(menu.tab(), 0);
        menu.handle_key(Key::Tab);
        assert_eq!(names(&menu), ["cloud-ts"]);
        menu.handle_key(Key::ShiftTab);
        assert_eq!(menu.tab(), 0);
        menu.handle_key(Key::ShiftTab);
        assert_eq!(menu.tab(), 4);
        menu.handle_key(Key::Tab);
        assert_eq!(menu.tab(), 0);
        menu.handle_key(Key::Digit(9));
        assert_eq!(menu.tab(), 0);
        assert_eq!(menu.handle_key(Key::Quit), Action::Exit(0));
        assert_eq!(menu.handle_key(Key::Interrupt), Action::Exit(130));
    }

    #[test]
    fn wide_tabs_and_hitboxes() {
        let mut menu = fixture();
        assert_eq!(menu.tabs()[0].text, " 0:All(6) ");
        assert_eq!(menu.tabs()[0].width, 10);
        assert_eq!(menu.tabs()[1].text, " 1:Tailscale(1) ");
        assert_eq!(menu.tabs()[1].width, 16);
        assert_eq!(menu.tabs()[2].width, 11);
        let layout = menu.layout(30, 160);
        assert_eq!(layout.header, Header::Tabs);
        assert_eq!(
            layout.tab_hitboxes[2],
            TabHitbox {
                tab: 2,
                start: 27,
                end: 37
            }
        );
        assert_eq!(click(&mut menu, 29, 1, 0), Action::None);
        assert_eq!(names(&menu), ["office-ts", "work-a", "work-b"]);
        assert_eq!(cell_width("工作e\u{301}"), 5);
        assert_eq!(truncate_cells("工作e\u{301}", 3), "工");
        assert_eq!(truncate_cells("工作e\u{301}", 5), "工作e\u{301}");
    }

    #[test]
    fn more_than_nine_groups_and_empty_tabs() {
        let mut text = (0..11).map(|i| format!("group|G{i}\n")).collect::<String>();
        text.push_str("match|G10|alias|*\n");
        let config = config::parse(Path::new("rules"), &text).unwrap();
        let (_, mut hosts) = hosts::tests::fixture();
        for host in &mut hosts {
            host.group = hosts::classify(&config, &host.alias, &host.info.hostname);
        }
        let mut menu = Menu::new(config.labels, hosts);
        assert_eq!(menu.tabs().len(), 13);
        assert_eq!(menu.shortcut_max(), 9);
        menu.handle_key(Key::Left);
        assert_eq!(menu.tab(), 12);
        assert_eq!(menu.selected_host(), None);
        assert_eq!(menu.handle_key(Key::Enter), Action::None);
        menu.handle_key(Key::Down);
        assert_eq!(menu.selection(), 0);
        menu.handle_key(Key::Left);
        assert_eq!(
            menu.layout(8, 40).header,
            Header::Compact("11:G10 (6)".into())
        );
        assert_eq!(menu.layout(1, 40).header, Header::Hidden);
        assert_eq!(names(&menu)[0], "cloud-ts");
        assert_eq!(
            menu.layout(8, 40).header,
            Header::Compact("11:G10 (6)".into())
        );
        menu.handle_key(Key::Digit(10));
        assert_eq!(menu.tab(), 11);
        menu.handle_key(Key::Digit(9));
        assert_eq!(menu.tab(), 9);
        assert!(menu.visible().is_empty());
        let layout = menu.layout(8, 40);
        assert_eq!(layout.offset, 0);
        assert_eq!(layout.selected_row, 0);
        assert_eq!(
            layout.footer.as_deref(),
            Some("sshto  0/0  Click: select  Double-click")
        );
    }

    #[test]
    fn layout_height_width_thresholds_and_reserved_column() {
        for (rows, headers, footers, slots) in [
            (0, 0, 0, 1),
            (1, 0, 0, 1),
            (2, 1, 0, 1),
            (3, 1, 1, 1),
            (4, 1, 1, 2),
            (5, 2, 1, 2),
            (7, 2, 1, 4),
        ] {
            let mut menu = fixture();
            let layout = menu.layout(rows, 0);
            assert_eq!(
                (layout.header_rows, layout.footer_rows, layout.slots),
                (headers, footers, slots)
            );
            assert_eq!(
                (layout.rows, layout.cols, layout.width),
                (rows.max(1), 2, 1)
            );
            assert!(layout.tab_hitboxes.is_empty());
        }
        let mut menu = fixture();
        let total_width: usize = menu.tabs().iter().map(|tab| tab.width).sum();
        assert_eq!(menu.layout(5, total_width + 1).header, Header::Tabs);
        assert!(matches!(
            menu.layout(5, total_width).header,
            Header::Compact(_)
        ));
        assert_eq!(menu.layout(5, 23).header, Header::Compact("0: (6)".into()));
        assert_eq!(
            menu.layout(5, 24).header,
            Header::Compact("0:All (6)".into())
        );
        assert_eq!(
            menu.layout(5, 77).controls.as_deref(),
            Some("< > Tab: group  Up/Down  Enter  q/Esc")
        );
        assert!(menu
            .layout(5, 78)
            .controls
            .unwrap()
            .starts_with("  Left/Right Tab: group  0-4: jump"));
        let layout = menu.layout(30, 160);
        assert_eq!(
            (layout.indent, layout.alias_width, layout.info_width),
            (2, 9, 142)
        );
        menu.select_tab(2);
        let layout = menu.layout(30, 160);
        assert_eq!(
            (layout.indent, layout.alias_width, layout.info_width),
            (0, 9, 144)
        );
    }

    #[test]
    fn scroll_restores_heading_above_first_host_and_clamps_after_resize() {
        let mut menu = fixture();
        assert_eq!(menu.layout(7, 40).offset, 0);
        for expected in [0, 1, 2, 4, 6] {
            menu.handle_key(Key::Down);
            assert_eq!(menu.layout(7, 40).offset, expected);
        }
        menu.handle_key(Key::Up);
        assert_eq!(menu.layout(7, 40).offset, 6);
        menu.handle_key(Key::Up);
        assert_eq!(menu.layout(7, 40).offset, 5);
        menu.handle_key(Key::Up);
        assert_eq!(menu.layout(7, 40).offset, 4);
        menu.handle_key(Key::Up);
        assert_eq!(menu.layout(7, 40).offset, 2);
        assert_eq!(menu.display_rows()[2], DisplayRow::Heading(1));
        let layout = menu.layout(1, 40);
        assert_eq!((layout.offset, layout.selected_row), (3, 3));
        assert_eq!(menu.layout(30, 40).offset, 0);
        menu.select_tab(0);
        assert_eq!(menu.layout(1, 40).offset, 1);
        menu.handle_key(Key::Down);
        assert_eq!(menu.layout(1, 40).offset, 3);
        menu.handle_key(Key::Up);
        assert_eq!(menu.layout(1, 40).offset, 1);
    }

    #[test]
    fn double_click_requires_release_on_same_row_within_450_ms() {
        let mut menu = fixture();
        menu.select_tab(2);
        menu.layout(30, 160);
        assert_eq!(click(&mut menu, 5, 4, 1000), Action::None);
        assert_eq!(menu.selected_host(), Some(4));
        assert_eq!(
            mouse(&mut menu, 0, 5, 4, MousePhase::Press, 1450),
            Action::None
        );
        assert_eq!(
            mouse(&mut menu, 0, 20, 4, MousePhase::Release, 1450),
            Action::Connect(4)
        );
        menu.handle_key(Key::Other);
        assert_eq!(click(&mut menu, 5, 4, 2000), Action::None);
        assert_eq!(click(&mut menu, 5, 4, 2451), Action::None);
        assert_eq!(click(&mut menu, 5, 5, 2500), Action::None);
        assert_eq!(
            mouse(&mut menu, 0, 5, 5, MousePhase::Press, 2600),
            Action::None
        );
        assert_eq!(
            mouse(&mut menu, 0, 5, 4, MousePhase::Release, 2600),
            Action::None
        );
        assert_eq!(click(&mut menu, 5, 5, 2700), Action::None);
        menu.handle_key(Key::Up);
        menu.handle_key(Key::Down);
        assert_eq!(click(&mut menu, 5, 5, 2800), Action::None);
        assert_eq!(
            mouse(&mut menu, 0, 5, 5, MousePhase::Release, 2801),
            Action::None
        );
        assert_eq!(click(&mut menu, 5, 5, 2802), Action::None);
    }

    #[test]
    fn headings_wheels_blank_rows_and_outside_clicks() {
        let mut menu = fixture();
        menu.layout(7, 40);
        assert_eq!(click(&mut menu, 4, 3, 0), Action::None);
        assert_eq!(click(&mut menu, 4, 3, 1), Action::None);
        assert_eq!(menu.selection(), 0);
        for index in 1..=5 {
            assert_eq!(
                mouse(&mut menu, 65, 5, 4, MousePhase::Press, index),
                Action::None
            );
            assert_eq!(menu.selection(), index as usize);
            menu.layout(7, 40);
        }
        mouse(&mut menu, 65, 5, 4, MousePhase::Press, 6);
        assert_eq!(menu.selection(), 5);
        mouse(&mut menu, 64, 5, 4, MousePhase::Release, 7);
        assert_eq!(menu.selection(), 5);
        for _ in 0..8 {
            mouse(&mut menu, 64, 5, 4, MousePhase::Press, 8);
        }
        assert_eq!(menu.selection(), 0);
        menu.layout(7, 40);
        for (x, y) in [(0, 4), (40, 4), (41, 4), (5, 0), (5, 8), (5, 2), (5, 7)] {
            assert_eq!(click(&mut menu, x, y, 10), Action::None);
            mouse(&mut menu, 65, x, y, MousePhase::Press, 10);
            assert_eq!(menu.selection(), 0);
        }
        assert_eq!(click(&mut menu, 5, 4, 100), Action::None);
        assert_eq!(click(&mut menu, 5, 3, 101), Action::None);
        assert_eq!(click(&mut menu, 5, 4, 102), Action::None);
        mouse(&mut menu, 64, 5, 4, MousePhase::Press, 103);
        assert_eq!(click(&mut menu, 5, 4, 104), Action::None);
        menu.select_tab(1);
        menu.layout(30, 160);
        assert_eq!(click(&mut menu, 5, 3, 200), Action::None);
        assert_eq!(click(&mut menu, 5, 4, 201), Action::None);
        assert_eq!(click(&mut menu, 5, 3, 202), Action::None);
    }

    #[test]
    fn compact_headers_do_not_have_clickable_tabs_and_resize_discards_press() {
        let mut menu = fixture();
        menu.layout(8, 40);
        assert_eq!(click(&mut menu, 29, 1, 0), Action::None);
        assert_eq!(menu.tab(), 0);
        mouse(&mut menu, 0, 5, 4, MousePhase::Press, 1);
        menu.mark_resized();
        mouse(&mut menu, 0, 5, 4, MousePhase::Release, 2);
        menu.layout(30, 160);
        assert_eq!(
            mouse(&mut menu, 0, 5, 4, MousePhase::Release, 3),
            Action::None
        );
        assert_eq!(click(&mut menu, 5, 4, 4), Action::None);
        menu.select_tab(0);
        assert_eq!(click(&mut menu, 5, 4, 5), Action::None);
    }

    #[test]
    fn resize_discards_press_even_when_redrawn_before_release() {
        let mut menu = fixture();
        menu.select_tab(2);
        menu.layout(30, 160);
        mouse(&mut menu, 0, 5, 4, MousePhase::Press, 1);
        menu.mark_resized();
        menu.layout(30, 100);
        assert_eq!(
            mouse(&mut menu, 0, 5, 4, MousePhase::Release, 2),
            Action::None
        );
        assert_eq!(click(&mut menu, 5, 4, 3), Action::None);
    }
}
