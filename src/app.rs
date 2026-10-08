//! Hardware-independent application navigation state.

use alloc::{string::String, vec::Vec};

use crate::{
    catalog::{visible_entries, AppEntry, Filter, SortOrder},
    text_layout::wrap_lines,
};

/// Converts pointer movement into scroll pixels for a pressed memo area.
/// Any scrollable memo can use this tracker; the view owns its scroll offset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MemoDrag {
    last_y: Option<i16>,
    pixel_remainder: i16,
    active: bool,
}

impl MemoDrag {
    fn update(&mut self, started: bool, pointer_y: Option<i16>, held: bool) -> i16 {
        let Some(pointer_y) = pointer_y.filter(|_| held || started) else {
            *self = Self::default();
            return 0;
        };
        if started {
            self.active = true;
            self.last_y = Some(pointer_y);
            self.pixel_remainder = 0;
            return 0;
        }
        if !held || !self.active {
            return 0;
        }
        let Some(previous_y) = self.last_y.replace(pointer_y) else {
            return 0;
        };
        let delta_y = i32::from(pointer_y) - i32::from(previous_y);
        let scaled = i32::from(self.pixel_remainder) - delta_y;
        let scroll_pixels = scaled / 32;
        self.pixel_remainder = (scaled % 32) as i16;
        scroll_pixels.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Screen {
    #[default]
    Browser,
    MainMenu,
    AppDetails,
    DeleteConfirm,
    WiiloadConfirm,
    WiiloadInstalling,
    WiiloadReceiving,
    Options,
    NetworkSettings,
    NetworkScan,
    NetworkKeyboard,
    About,
    Notice,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NoticeKind {
    #[default]
    Generic,
    Information,
    Warning,
    Error,
}

/// Input after the platform layer maps controller buttons to logical actions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Input {
    Home,
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    ToggleBrowseMode,
    OpenOptions,
    PreviousPage,
    NextPage,
}

/// An interactive item under the Wii Remote pointer. Targets are semantic;
/// the renderer supplies their bounds from the same layout it draws.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerTarget {
    App(usize),
    Page { forward: bool },
    Menu(usize),
    Detail(usize),
    DetailMemo,
    NoticeMemo,
    WiiloadMemo,
    DeleteChoice { yes: bool },
    WiiloadChoice { yes: bool },
    Option(usize),
    NetworkOption(usize),
    KeyboardKey(usize),
    NoticeConfirm,
}

/// Work for a hardware or service layer to perform after a state transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Action {
    #[default]
    None,
    LaunchSelected,
    ApplyTheme,
    DeleteSelected,
    LaunchBootMii,
    Exit,
    Shutdown,
    PreviousPage,
    NextPage,
    ApplyOptions,
    LoadNetworkConfig,
    SaveNetworkConfig,
    ScanNetworks,
    AcceptWiiloadZip,
    RejectWiiloadZip,
}

/// Main menu focus follows the options available on this console.
const MENU_ITEMS_WITHOUT_BOOTMII: usize = 4;
const MENU_ITEMS_WITH_BOOTMII: usize = 5;
const DEFAULT_BROWSER_ROWS: usize = 5;
pub const DETAIL_DESCRIPTION_LINE_WIDTH: usize = 76;
pub const DETAIL_DESCRIPTION_VISIBLE_LINES: usize = 16;
pub const MESSAGE_MEMO_VISIBLE_LINES: usize = 20;
pub const ABOUT_MEMO_LINE_WIDTH: usize = 76;
pub const ABOUT_MEMO_VISIBLE_LINES: usize = 20;
pub const NETWORK_KEYBOARD_COLUMNS: usize = 10;
pub const NETWORK_KEYBOARD_KEYS: usize = 46;
const KEYBOARD_ALPHA: [&[u8; 10]; 4] = [b"1234567890", b"qwertyuiop", b"asdfghjkl;", b"zxcvbnm,./"];
const KEYBOARD_SYMBOL: [&[u8; 10]; 4] =
    [b"!@#$%^&*()", b"`~[]{}<>?/", b"\\|:;\"'=-_+", b"~`#$%^&*()"];
pub const ABOUT_CREDITS: &str = "Credits\n\nThis port is written in Rust and uses libogc-rs for Wii platform support.\n\nThe following credits are from the original C Homebrew Channel:\n\nThe Homebrew Channel was made possible by the following people:\n\ndhewg (EWRONGCHAN) - application code, geckoloader code\nblasty (ESTONED) - application code\nmarcan (EFAILURE) - reload stub, banner, installer, packaging\nbushing (EWANTPONY) - socket code, loader code\nsegher (EBUGFOUND) - nandloader stub code\nsouLLy (ENOTHERE) - banner graphics\ndrmr (EMORECOWBELL) - banner audio, channel graphics\nmha (E404) - update server and hosting\n\nThe original C HBC used devkitPPC and libogc, by shagkur, WinterMute, and everyone else who contributed.\n\nThanks to all the beta testers\n\nKind regards to the following people too:\n\nsepp256 - dropped some good GX hints\nchishm, svpe, rodries, hermes - libfat port\nalien - some graphics\njodi - the lulz\n\nThe following library credits are from the original C HBC:\n\nwiiuse - para's Wiimote library, then integrated with libogc\nlibpng - the official PNG library\nMini-XML - small and efficient XML parsing library\nFreeType - the free TrueType/OpenType font renderer\n";

/// Number of app columns and rows shown by the browser.
///
/// HBC uses five rows in both list and grid modes. A one-column layout is a
/// list; wider layouts use directional grid movement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowserLayout {
    pub columns: usize,
    pub rows: usize,
}

impl Default for BrowserLayout {
    fn default() -> Self {
        Self {
            columns: 1,
            rows: DEFAULT_BROWSER_ROWS,
        }
    }
}

impl BrowserLayout {
    fn page_capacity(self) -> usize {
        self.columns.max(1).saturating_mul(self.rows.max(1))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageNavigation {
    pub start: usize,
    pub capacity: usize,
    pub visible_count: usize,
}

impl PageNavigation {
    pub fn has_previous(self) -> bool {
        self.start > 0
    }

    pub fn has_next(self) -> bool {
        self.start.saturating_add(self.capacity) < self.visible_count
    }
}

/// Build the four-device status snapshot. An already mounted active device is
/// available even if an alternate-device probe failed.
pub fn device_availability_snapshot(
    active_mounted: Option<usize>,
    detected: [bool; 4],
) -> [bool; 4] {
    let mut snapshot = detected;
    if let Some(active) = active_mounted.filter(|index| *index < snapshot.len()) {
        snapshot[active] = true;
    }
    snapshot
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppController {
    pub screen: Screen,
    pub selected_app: Option<String>,
    pub filter: Filter,
    pub sort: SortOrder,
    selected_index: usize,
    layout: BrowserLayout,
    grid_columns: usize,
    menu_focus: usize,
    detail_focus: usize,
    detail_description_scroll: usize,
    detail_description_max_scroll: usize,
    about_scroll: usize,
    about_scroll_ticks: usize,
    measured_memo: Option<(Screen, usize, u16)>,
    about_has_theme_description: bool,
    detail_scroll_app: Option<String>,
    detail_scroll_pixel_remainder: i16,
    memo_drag: MemoDrag,
    memo_drag_target: Option<PointerTarget>,
    delete_confirmation_yes: bool,
    wiiload_confirmation_yes: bool,
    wiiload_confirmation_text: String,
    wiiload_confirmation_scroll: usize,
    wiiload_confirmation_max_scroll: usize,
    wiiload_confirmation_scroll_pixel_remainder: i16,
    wiiload_install_progress: u8,
    notice: Option<String>,
    notice_kind: NoticeKind,
    notice_scroll: usize,
    notice_scroll_max: usize,
    notice_scroll_pixel_remainder: i16,
    notice_return_screen: Screen,
    bootmii_available: bool,
    preferred_device: i32,
    active_device: Option<i32>,
    device_available: [Option<bool>; 4],
    pending_device: i32,
    pending_sort: SortOrder,
    options_focus: usize,
    network_config: Option<crate::network_config::NetworkConfig>,
    network_ssid: Vec<u8>,
    network_key: Vec<u8>,
    network_encryption: u8,
    network_focus: usize,
    scanned_networks: crate::network_config::ScannedNetworks,
    network_scan_focus: usize,
    keyboard_focus: usize,
    keyboard_symbols: bool,
    keyboard_shift: bool,
    keyboard_target_key: bool,
    pointer_target: Option<PointerTarget>,
}

impl AppController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the catalog in the active filter and sort order.
    pub fn visible_apps<'a>(&self, catalog: &'a [AppEntry]) -> Vec<&'a AppEntry> {
        visible_entries(catalog, self.filter, self.sort)
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn menu_focus(&self) -> usize {
        self.menu_focus
    }

    pub fn detail_focus(&self) -> usize {
        self.detail_focus
    }

    pub fn request_wiiload_zip_confirmation(&mut self) {
        self.wiiload_confirmation_yes = false;
        self.wiiload_confirmation_text = String::from("Extract the received ZIP file?");
        self.wiiload_confirmation_scroll = 0;
        self.wiiload_confirmation_max_scroll = 0;
        self.wiiload_confirmation_scroll_pixel_remainder = 0;
        self.screen = Screen::WiiloadConfirm;
    }

    pub fn set_wiiload_confirmation_text(&mut self, text: String) {
        self.wiiload_confirmation_text = text;
        self.wiiload_confirmation_scroll = 0;
        self.wiiload_confirmation_scroll_pixel_remainder = 0;
        self.wiiload_confirmation_max_scroll = wrap_lines(&self.wiiload_confirmation_text, 74)
            .len()
            .saturating_sub(MESSAGE_MEMO_VISIBLE_LINES);
    }

    pub fn wiiload_confirmation_text(&self) -> &str {
        &self.wiiload_confirmation_text
    }

    pub fn wiiload_confirmation_scroll(&self) -> usize {
        self.wiiload_confirmation_scroll
    }

    pub fn wiiload_confirmation_max_scroll(&self) -> usize {
        self.wiiload_confirmation_max_scroll
    }

    pub fn wiiload_confirmation_yes(&self) -> bool {
        self.wiiload_confirmation_yes
    }

    pub fn show_wiiload_installing(&mut self) {
        self.wiiload_install_progress = 0;
        self.screen = Screen::WiiloadInstalling;
    }

    pub fn show_wiiload_receiving(&mut self) {
        self.wiiload_install_progress = 0;
        self.screen = Screen::WiiloadReceiving;
    }

    pub fn finish_wiiload_receiving(&mut self) {
        if self.screen == Screen::WiiloadReceiving {
            self.screen = Screen::Browser;
        }
    }

    pub fn set_wiiload_install_progress(&mut self, percent: u8) {
        if matches!(
            self.screen,
            Screen::WiiloadInstalling | Screen::WiiloadReceiving
        ) {
            self.wiiload_install_progress = percent.min(100);
        }
    }

    pub fn wiiload_install_progress(&self) -> u8 {
        self.wiiload_install_progress
    }

    pub fn finish_wiiload_installing(&mut self) {
        if self.screen == Screen::WiiloadInstalling {
            self.screen = Screen::Browser;
        }
    }

    pub fn detail_description_scroll(&self) -> usize {
        self.detail_description_scroll
    }

    pub fn detail_description_max_scroll(&self) -> usize {
        self.detail_description_max_scroll
    }

    pub fn scroll_detail_by_stick(&mut self, pixels: i16) {
        self.scroll_detail_by_pixels(pixels);
    }

    pub fn scroll_active_memo_by_stick(&mut self, pixels: i16) {
        match self.screen {
            Screen::AppDetails => self.scroll_detail_by_pixels(pixels),
            Screen::Notice => self.scroll_notice_by_pixels(pixels),
            Screen::About => {}
            Screen::WiiloadConfirm => self.scroll_wiiload_confirmation_by_pixels(pixels),
            _ => {}
        }
    }

    pub fn about_scroll(&self) -> usize {
        self.about_scroll
    }

    pub fn set_about_theme_description(&mut self, available: bool) {
        self.about_has_theme_description = available;
        self.about_scroll = self.about_scroll.min(self.about_max_scroll());
    }

    fn about_max_scroll(&self) -> usize {
        if let Some((Screen::About, max_scroll, _)) = self.measured_memo {
            return max_scroll;
        }
        let lines = crate::text_layout::wrap_memo_lines(ABOUT_CREDITS, ABOUT_MEMO_LINE_WIDTH)
            .len()
            .saturating_add(if self.about_has_theme_description {
                1
            } else {
                0
            });
        lines.saturating_sub(ABOUT_MEMO_VISIBLE_LINES)
    }

    pub fn set_measured_memo(&mut self, max_scroll: usize, line_height: u16) {
        self.measured_memo = Some((self.screen, max_scroll, line_height.max(1)));
        match self.screen {
            Screen::About => self.about_scroll = self.about_scroll.min(max_scroll),
            Screen::AppDetails => {
                self.detail_description_max_scroll = max_scroll;
                self.detail_description_scroll = self.detail_description_scroll.min(max_scroll);
            }
            Screen::Notice => {
                self.notice_scroll_max = max_scroll;
                self.notice_scroll = self.notice_scroll.min(max_scroll);
            }
            Screen::WiiloadConfirm => {
                self.wiiload_confirmation_max_scroll = max_scroll;
                self.wiiload_confirmation_scroll = self.wiiload_confirmation_scroll.min(max_scroll);
            }
            _ => {}
        }
    }

    fn memo_line_height(&self) -> i16 {
        self.measured_memo
            .filter(|(screen, _, _)| *screen == self.screen)
            .map_or(9, |(_, _, height)| height.min(i16::MAX as u16) as i16)
    }

    /// Match the C About memo's delayed, slow upward crawl.
    pub fn tick_about_scroll(&mut self) {
        if self.screen != Screen::About {
            return;
        }
        self.about_scroll_ticks = self.about_scroll_ticks.saturating_add(1);
        if self.about_scroll_ticks > 120 && self.about_scroll_ticks % 3 == 0 {
            let max = self.about_max_scroll();
            self.about_scroll = self.about_scroll.saturating_add(1).min(max);
        }
    }

    fn scroll_detail_by_pixels(&mut self, pixels: i16) {
        if self.screen != Screen::AppDetails {
            return;
        }
        self.detail_scroll_pixel_remainder =
            self.detail_scroll_pixel_remainder.saturating_add(pixels);
        let line_height = self.memo_line_height();
        let lines = self.detail_scroll_pixel_remainder / line_height;
        self.detail_scroll_pixel_remainder %= line_height;
        if lines > 0 {
            self.detail_description_scroll = self
                .detail_description_scroll
                .saturating_sub(lines as usize)
                .min(self.detail_description_max_scroll);
        } else if lines < 0 {
            self.detail_description_scroll = self
                .detail_description_scroll
                .saturating_add(lines.unsigned_abs() as usize)
                .min(self.detail_description_max_scroll);
        }
    }

    pub fn delete_confirmation_yes(&self) -> bool {
        self.delete_confirmation_yes
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn notice_kind(&self) -> NoticeKind {
        self.notice_kind
    }

    pub fn notice_scroll(&self) -> usize {
        self.notice_scroll
    }

    pub fn notice_scroll_max(&self) -> usize {
        self.notice_scroll_max
    }

    fn scroll_notice_by_pixels(&mut self, pixels: i16) {
        self.notice_scroll_pixel_remainder =
            self.notice_scroll_pixel_remainder.saturating_add(pixels);
        let line_height = self.memo_line_height();
        let lines = self.notice_scroll_pixel_remainder / line_height;
        self.notice_scroll_pixel_remainder %= line_height;
        if lines > 0 {
            self.notice_scroll = self
                .notice_scroll
                .saturating_add(lines.unsigned_abs() as usize)
                .min(self.notice_scroll_max);
        } else if lines < 0 {
            self.notice_scroll = self
                .notice_scroll
                .saturating_sub(lines.unsigned_abs() as usize);
        }
    }

    fn scroll_wiiload_confirmation_by_pixels(&mut self, pixels: i16) {
        self.wiiload_confirmation_scroll_pixel_remainder = self
            .wiiload_confirmation_scroll_pixel_remainder
            .saturating_add(pixels);
        let line_height = self.memo_line_height();
        let lines = self.wiiload_confirmation_scroll_pixel_remainder / line_height;
        self.wiiload_confirmation_scroll_pixel_remainder %= line_height;
        if lines > 0 {
            self.wiiload_confirmation_scroll = self
                .wiiload_confirmation_scroll
                .saturating_add(lines as usize)
                .min(self.wiiload_confirmation_max_scroll);
        } else if lines < 0 {
            self.wiiload_confirmation_scroll = self
                .wiiload_confirmation_scroll
                .saturating_sub(lines.unsigned_abs() as usize);
        }
    }

    /// Show a message and return to the current screen after dismissal.
    pub fn show_notice(&mut self, message: String) {
        self.show_notice_with_kind(message, NoticeKind::Generic);
    }

    pub fn show_notice_with_kind(&mut self, message: String, kind: NoticeKind) {
        if self.screen != Screen::Notice {
            self.notice_return_screen = self.screen;
        }
        self.notice_scroll = 0;
        self.notice_scroll_pixel_remainder = 0;
        self.notice = Some(message);
        self.notice_kind = kind;
        self.notice_scroll_max = self.notice.as_deref().map_or(0, |value| {
            wrap_lines(value, 74)
                .len()
                .saturating_sub(MESSAGE_MEMO_VISIBLE_LINES)
        });
        self.screen = Screen::Notice;
    }

    pub fn set_bootmii_available(&mut self, available: bool) {
        self.bootmii_available = available;
        let last_focus = self.menu_item_count().saturating_sub(1);
        self.menu_focus = self.menu_focus.min(last_focus);
    }

    pub fn bootmii_available(&self) -> bool {
        self.bootmii_available
    }

    pub fn preferred_device(&self) -> i32 {
        self.preferred_device
    }

    pub fn options_focus(&self) -> usize {
        self.options_focus
    }

    pub fn network_config(&self) -> Option<&crate::network_config::NetworkConfig> {
        self.network_config.as_ref()
    }

    pub fn network_ssid(&self) -> &[u8] {
        &self.network_ssid
    }

    pub fn network_key(&self) -> &[u8] {
        &self.network_key
    }

    pub fn network_encryption(&self) -> u8 {
        self.network_encryption
    }

    pub fn network_focus(&self) -> usize {
        self.network_focus
    }

    pub fn keyboard_focus(&self) -> usize {
        self.keyboard_focus
    }

    pub fn keyboard_target_key(&self) -> bool {
        self.keyboard_target_key
    }

    pub fn keyboard_char(&self, index: usize) -> Option<char> {
        if index >= 40 {
            return None;
        }
        let row = index / NETWORK_KEYBOARD_COLUMNS;
        let column = index % NETWORK_KEYBOARD_COLUMNS;
        let byte = if self.keyboard_symbols {
            KEYBOARD_SYMBOL[row][column]
        } else {
            KEYBOARD_ALPHA[row][column]
        };
        Some(if self.keyboard_shift && byte.is_ascii_lowercase() {
            byte.to_ascii_uppercase() as char
        } else {
            byte as char
        })
    }

    pub fn keyboard_command_label(&self, index: usize) -> &'static str {
        match index {
            40 => {
                if self.keyboard_symbols {
                    "ABC"
                } else {
                    "123"
                }
            }
            41 => {
                if self.keyboard_shift {
                    "SHIFT*"
                } else {
                    "SHIFT"
                }
            }
            42 => "SPACE",
            43 => "BACK",
            44 => "CLEAR",
            45 => "DONE",
            _ => "",
        }
    }

    pub fn set_network_config(&mut self, config: crate::network_config::NetworkConfig) {
        let profile_index = config.active_profile().unwrap_or(0);
        let wifi = config
            .profile(profile_index)
            .expect("profile index is valid")
            .wifi();
        let ssid_len = usize::from(wifi.ssid_len()).min(32);
        let key_len = usize::from(wifi.key_len()).min(64);
        self.network_ssid.clear();
        self.network_ssid
            .extend_from_slice(&wifi.ssid()[..ssid_len]);
        self.network_key.clear();
        self.network_key.extend_from_slice(&wifi.key()[..key_len]);
        self.network_encryption = wifi.encryption();
        self.network_config = Some(config);
        self.network_focus = 0;
        self.keyboard_focus = 0;
        self.keyboard_symbols = false;
        self.keyboard_shift = false;
        self.screen = Screen::NetworkSettings;
    }

    pub fn set_scanned_networks(&mut self, networks: crate::network_config::ScannedNetworks) {
        self.scanned_networks = networks;
        self.network_scan_focus = 0;
        self.screen = Screen::NetworkScan;
    }

    pub fn scanned_networks(&self) -> &[crate::network_config::ScannedNetwork] {
        self.scanned_networks.as_slice()
    }

    pub fn network_scan_focus(&self) -> usize {
        self.network_scan_focus
    }

    pub fn network_config_for_save(
        &self,
    ) -> Result<crate::network_config::NetworkConfig, NetworkConfigEditError> {
        let mut config = self
            .network_config
            .clone()
            .ok_or(NetworkConfigEditError::MissingSnapshot)?;
        let profile_index = config
            .active_wifi_profile()
            .map_err(NetworkConfigEditError::Profile)?;
        let mut profile = config
            .profile_mut(profile_index)
            .expect("profile index is valid");
        let mut wifi = profile.wifi_mut();
        if !wifi.set_ssid(&self.network_ssid)
            || !wifi.set_encryption(self.network_encryption)
            || !wifi.set_key(&self.network_key)
        {
            return Err(NetworkConfigEditError::Wifi(
                crate::network_config::WifiValidationError::InvalidWpaKey,
            ));
        }
        drop(wifi);
        crate::network_config::validate_wifi(config.profile(profile_index).unwrap().wifi())
            .map_err(NetworkConfigEditError::Wifi)?;
        Ok(config)
    }

    pub fn network_config_snapshot(&self) -> Option<&crate::network_config::NetworkConfig> {
        self.network_config.as_ref()
    }

    pub fn clear_network_config(&mut self) {
        self.network_config = None;
    }

    pub fn pointer_target(&self) -> Option<PointerTarget> {
        self.pointer_target
    }

    pub fn pending_device(&self) -> i32 {
        self.pending_device
    }

    pub fn pending_sort(&self) -> SortOrder {
        self.pending_sort
    }

    pub fn set_preferred_device(&mut self, device: i32) {
        self.preferred_device = device;
    }

    pub fn set_active_device(&mut self, device: Option<i32>) {
        self.active_device = device;
    }

    pub fn device_available(&self, device: usize) -> Option<bool> {
        self.device_available.get(device).copied().flatten()
    }

    pub fn set_device_availability(&mut self, availability: [bool; 4]) {
        self.device_available = availability.map(Some);
    }

    pub fn menu_item_count(&self) -> usize {
        if self.bootmii_available {
            MENU_ITEMS_WITH_BOOTMII
        } else {
            MENU_ITEMS_WITHOUT_BOOTMII
        }
    }

    /// Keep the selected app identity valid after a catalog refresh.
    pub fn reconcile_catalog(&mut self, catalog: &[AppEntry]) {
        self.reconcile_selection(catalog);
        self.refresh_detail_scroll_limit(catalog);
    }

    pub fn browser_layout(&self) -> BrowserLayout {
        self.layout
    }

    /// Set list or grid dimensions without losing the selected app identity.
    pub fn set_browser_layout(&mut self, layout: BrowserLayout, catalog: &[AppEntry]) {
        self.layout = BrowserLayout {
            columns: layout.columns.max(1),
            rows: layout.rows.max(1),
        };
        self.reconcile_selection(catalog);
    }

    /// Set the grid width used when the user toggles from list mode.
    pub fn set_grid_columns(&mut self, columns: usize, catalog: &[AppEntry]) {
        self.grid_columns = columns.max(3);
        if self.layout.columns > 1 {
            self.set_browser_layout(
                BrowserLayout {
                    columns: self.grid_columns,
                    rows: self.layout.rows,
                },
                catalog,
            );
        }
    }

    /// Index of the first entry on the selected app's page.
    pub fn page_start(&self) -> usize {
        self.selected_index / self.layout.page_capacity() * self.layout.page_capacity()
    }

    pub fn page_navigation(&self, catalog: &[AppEntry]) -> PageNavigation {
        self.page_navigation_for_count(self.visible_apps(catalog).len())
    }

    fn page_navigation_for_count(&self, visible_count: usize) -> PageNavigation {
        PageNavigation {
            start: self.page_start(),
            capacity: self.layout.page_capacity(),
            visible_count,
        }
    }

    /// Return the current page in the active filter and sort order.
    pub fn visible_page<'a>(&self, catalog: &'a [AppEntry]) -> Vec<&'a AppEntry> {
        let visible = self.visible_apps(catalog);
        let start = self.page_start().min(visible.len());
        let end = start
            .saturating_add(self.layout.page_capacity())
            .min(visible.len());
        visible[start..end].to_vec()
    }

    /// Apply one logical input and keep the selected identity valid for the
    /// current visible catalog. Selection does not wrap at either end.
    pub fn handle(&mut self, input: Input, catalog: &[AppEntry]) -> Action {
        self.reconcile_selection(catalog);
        self.refresh_detail_scroll_limit(catalog);

        match self.screen {
            Screen::Browser => self.handle_browser(input, catalog),
            Screen::MainMenu => self.handle_menu(input),
            Screen::AppDetails => match input {
                Input::Back => {
                    self.screen = Screen::Browser;
                    Action::None
                }
                Input::Left => {
                    self.move_detail_focus(false);
                    Action::None
                }
                Input::Right => {
                    self.move_detail_focus(true);
                    Action::None
                }
                Input::Up => {
                    self.detail_description_scroll =
                        self.detail_description_scroll.saturating_sub(1);
                    Action::None
                }
                Input::Down => {
                    self.detail_description_scroll = self
                        .detail_description_scroll
                        .saturating_add(1)
                        .min(self.detail_description_max_scroll);
                    Action::None
                }
                Input::Confirm if self.selected_app.is_some() => match self.detail_focus {
                    8 => {
                        self.delete_confirmation_yes = false;
                        self.screen = Screen::DeleteConfirm;
                        Action::None
                    }
                    9 => {
                        let is_theme = self
                            .selected_app
                            .as_deref()
                            .and_then(|selected| {
                                catalog.iter().find(|entry| entry.directory == selected)
                            })
                            .is_some_and(|entry| entry.kind == crate::catalog::AppKind::Theme);
                        if is_theme {
                            self.screen = Screen::Browser;
                            Action::ApplyTheme
                        } else {
                            Action::LaunchSelected
                        }
                    }
                    10 => {
                        self.screen = Screen::Browser;
                        Action::None
                    }
                    _ => Action::None,
                },
                _ => Action::None,
            },
            Screen::DeleteConfirm => match input {
                Input::Left | Input::Right => {
                    self.delete_confirmation_yes = !self.delete_confirmation_yes;
                    Action::None
                }
                Input::Back => {
                    self.screen = Screen::AppDetails;
                    Action::None
                }
                Input::Confirm if self.delete_confirmation_yes => {
                    self.screen = Screen::Browser;
                    Action::DeleteSelected
                }
                Input::Confirm => {
                    self.screen = Screen::AppDetails;
                    Action::None
                }
                _ => Action::None,
            },
            Screen::WiiloadConfirm => match input {
                Input::Left | Input::Right => {
                    self.wiiload_confirmation_yes = !self.wiiload_confirmation_yes;
                    Action::None
                }
                Input::Up => {
                    self.wiiload_confirmation_scroll =
                        self.wiiload_confirmation_scroll.saturating_sub(1);
                    Action::None
                }
                Input::Down => {
                    self.wiiload_confirmation_scroll = self
                        .wiiload_confirmation_scroll
                        .saturating_add(1)
                        .min(self.wiiload_confirmation_max_scroll);
                    Action::None
                }
                Input::Back => {
                    self.screen = Screen::Browser;
                    Action::RejectWiiloadZip
                }
                Input::Home => {
                    self.screen = Screen::MainMenu;
                    self.menu_focus = 0;
                    Action::RejectWiiloadZip
                }
                Input::Confirm if self.wiiload_confirmation_yes => {
                    self.screen = Screen::Browser;
                    Action::AcceptWiiloadZip
                }
                Input::Confirm => {
                    self.screen = Screen::Browser;
                    Action::RejectWiiloadZip
                }
                _ => Action::None,
            },
            Screen::WiiloadInstalling | Screen::WiiloadReceiving => Action::None,
            Screen::Options => self.handle_options(input, catalog),
            Screen::NetworkSettings => self.handle_network_settings(input),
            Screen::NetworkScan => self.handle_network_scan(input),
            Screen::NetworkKeyboard => self.handle_network_keyboard(input),
            Screen::About => match input {
                Input::Back | Input::Confirm => {
                    self.screen = Screen::MainMenu;
                    self.about_scroll = 0;
                    self.about_scroll_ticks = 0;
                    Action::None
                }
                Input::Up => {
                    self.about_scroll = self.about_scroll.saturating_sub(1);
                    Action::None
                }
                Input::Down => {
                    let max = self.about_max_scroll();
                    self.about_scroll = self.about_scroll.saturating_add(1).min(max);
                    Action::None
                }
                _ => Action::None,
            },
            Screen::Notice => match input {
                Input::Back | Input::Confirm | Input::Home => {
                    self.notice = None;
                    self.screen = self.notice_return_screen;
                    Action::None
                }
                Input::Up => {
                    self.notice_scroll = self.notice_scroll.saturating_sub(1);
                    Action::None
                }
                Input::Down => {
                    self.notice_scroll = self
                        .notice_scroll
                        .saturating_add(1)
                        .min(self.notice_scroll_max);
                    Action::None
                }
                _ => Action::None,
            },
        }
    }

    /// Move focus to an item selected by the renderer's hit map. A click then
    /// reuses the same Confirm and page actions as physical buttons.
    pub fn handle_pointer(
        &mut self,
        target: Option<PointerTarget>,
        activate: bool,
        catalog: &[AppEntry],
    ) -> Action {
        self.handle_pointer_frame(target, None, activate, false, catalog)
    }

    pub fn handle_pointer_frame(
        &mut self,
        target: Option<PointerTarget>,
        pointer_y: Option<i16>,
        pressed: bool,
        held: bool,
        catalog: &[AppEntry],
    ) -> Action {
        self.pointer_target = target;
        self.update_detail_drag(target, pointer_y, pressed, held);
        let Some(target) = target else {
            return Action::None;
        };
        self.reconcile_selection(catalog);
        match (self.screen, target) {
            (Screen::Browser, PointerTarget::App(index)) => {
                let visible = self.visible_apps(catalog);
                if let Some(entry) = visible.get(index) {
                    self.selected_app = Some(entry.directory.clone());
                    self.reconcile_selection(catalog);
                    if pressed {
                        return self.handle(Input::Confirm, catalog);
                    }
                }
                Action::None
            }
            (Screen::Browser, PointerTarget::Page { forward }) => {
                if pressed {
                    self.move_page(catalog, forward)
                } else {
                    Action::None
                }
            }
            (Screen::MainMenu, PointerTarget::Menu(index)) if index < self.menu_item_count() => {
                self.menu_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::AppDetails, PointerTarget::Detail(index)) if (8..=10).contains(&index) => {
                self.detail_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::AppDetails, PointerTarget::DetailMemo) => Action::None,
            (Screen::DeleteConfirm, PointerTarget::DeleteChoice { yes }) => {
                self.delete_confirmation_yes = yes;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::WiiloadConfirm, PointerTarget::WiiloadChoice { yes }) => {
                self.wiiload_confirmation_yes = yes;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::WiiloadConfirm, PointerTarget::WiiloadMemo) => Action::None,
            (Screen::WiiloadInstalling | Screen::WiiloadReceiving, _) => Action::None,
            (Screen::Options, PointerTarget::Option(index)) if self.option_enabled(index) => {
                self.options_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::NetworkSettings, PointerTarget::NetworkOption(index)) if index < 6 => {
                self.network_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::NetworkScan, PointerTarget::NetworkOption(index))
                if index < self.scanned_networks.len && index < 8 =>
            {
                self.network_scan_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::NetworkKeyboard, PointerTarget::KeyboardKey(index)) if index < 46 => {
                self.keyboard_focus = index;
                if pressed {
                    self.handle(Input::Confirm, catalog)
                } else {
                    Action::None
                }
            }
            (Screen::Notice, PointerTarget::NoticeConfirm) if pressed => {
                self.handle(Input::Confirm, catalog)
            }
            _ => Action::None,
        }
    }

    fn update_detail_drag(
        &mut self,
        target: Option<PointerTarget>,
        pointer_y: Option<i16>,
        pressed: bool,
        held: bool,
    ) {
        if pressed {
            self.memo_drag_target = match (self.screen, target) {
                (Screen::AppDetails, Some(PointerTarget::DetailMemo)) => {
                    Some(PointerTarget::DetailMemo)
                }
                (Screen::Notice, Some(PointerTarget::NoticeMemo)) => {
                    Some(PointerTarget::NoticeMemo)
                }
                (Screen::WiiloadConfirm, Some(PointerTarget::WiiloadMemo)) => {
                    Some(PointerTarget::WiiloadMemo)
                }
                _ => None,
            };
        }
        let drag_target = self.memo_drag_target;
        let detail_memo =
            self.screen == Screen::AppDetails && drag_target == Some(PointerTarget::DetailMemo);
        let notice_memo =
            self.screen == Screen::Notice && drag_target == Some(PointerTarget::NoticeMemo);
        let wiiload_memo = self.screen == Screen::WiiloadConfirm
            && drag_target == Some(PointerTarget::WiiloadMemo);
        let can_scroll = detail_memo || notice_memo || wiiload_memo;
        let scroll_pixels = self.memo_drag.update(
            can_scroll && pressed,
            pointer_y,
            (detail_memo || notice_memo || wiiload_memo || self.memo_drag.active) && held,
        );
        if detail_memo && scroll_pixels != 0 {
            self.scroll_detail_by_pixels(scroll_pixels);
        } else if (notice_memo || self.screen == Screen::Notice) && scroll_pixels != 0 {
            self.scroll_notice_by_pixels(scroll_pixels);
        } else if wiiload_memo && scroll_pixels != 0 {
            self.scroll_wiiload_confirmation_by_pixels(scroll_pixels);
        }
        if !held && !pressed {
            self.memo_drag_target = None;
        }
    }

    fn handle_browser(&mut self, input: Input, catalog: &[AppEntry]) -> Action {
        match input {
            Input::Home => {
                self.screen = Screen::MainMenu;
                self.menu_focus = 0;
                Action::None
            }
            Input::Up => {
                let step = if self.layout.columns == 1 {
                    1
                } else {
                    self.layout.columns
                };
                self.move_selection(catalog, false, step);
                Action::None
            }
            Input::Down => {
                let step = if self.layout.columns == 1 {
                    1
                } else {
                    self.layout.columns
                };
                self.move_selection(catalog, true, step);
                Action::None
            }
            Input::Left if self.layout.columns > 1 => {
                self.move_selection(catalog, false, 1);
                Action::None
            }
            Input::Right if self.layout.columns > 1 => {
                self.move_selection(catalog, true, 1);
                Action::None
            }
            Input::Confirm if self.selected_app.is_some() => {
                self.screen = Screen::AppDetails;
                self.detail_focus = 10;
                self.detail_description_scroll = 0;
                self.detail_description_max_scroll = 0;
                self.detail_scroll_app = None;
                self.detail_scroll_pixel_remainder = 0;
                self.refresh_detail_scroll_limit(catalog);
                Action::None
            }
            Input::ToggleBrowseMode => {
                self.layout.columns = if self.layout.columns == 1 {
                    self.grid_columns.max(3)
                } else {
                    1
                };
                self.reconcile_selection(catalog);
                Action::None
            }
            Input::OpenOptions => {
                self.pending_device = match self.active_device {
                    Some(device @ (0..=3)) => device,
                    _ => self.preferred_device.clamp(0, 3),
                };
                self.pending_sort = self.sort;
                self.options_focus = 7;
                self.screen = Screen::Options;
                Action::None
            }
            Input::PreviousPage => self.move_page(catalog, false),
            Input::NextPage => self.move_page(catalog, true),
            _ => Action::None,
        }
    }

    fn handle_menu(&mut self, input: Input) -> Action {
        match input {
            Input::Home | Input::Back => {
                self.screen = Screen::Browser;
                Action::None
            }
            Input::Up | Input::Left => {
                self.move_menu_focus(false);
                Action::None
            }
            Input::Down | Input::Right => {
                self.move_menu_focus(true);
                Action::None
            }
            Input::Confirm => match self.menu_focus {
                0 => {
                    self.screen = Screen::Browser;
                    Action::None
                }
                1 => {
                    self.screen = Screen::About;
                    Action::None
                }
                2 if self.bootmii_available => Action::LaunchBootMii,
                2 => Action::Exit,
                3 if self.bootmii_available => Action::Exit,
                3 => Action::Shutdown,
                4 => Action::Shutdown,
                _ => Action::None,
            },
            _ => Action::None,
        }
    }

    pub fn option_enabled(&self, index: usize) -> bool {
        match index {
            0..=3 => self.device_available(index) == Some(true),
            4..=8 => true,
            _ => false,
        }
    }

    fn move_options_focus(&mut self, input: Input) {
        const ROWS: [&[usize]; 5] = [&[0, 1], &[2, 3], &[4, 5], &[8], &[6, 7]];
        let row = ROWS
            .iter()
            .position(|items| items.contains(&self.options_focus))
            .unwrap_or(4);
        let column = usize::from(self.options_focus < 8 && self.options_focus % 2 != 0);
        match input {
            Input::Left | Input::Right => {
                let target = ROWS[row].get(usize::from(input == Input::Right)).copied();
                if let Some(target) = target.filter(|index| self.option_enabled(*index)) {
                    self.options_focus = target;
                }
            }
            Input::Up | Input::Down => {
                let step: isize = if input == Input::Up { -1 } else { 1 };
                let mut next = row as isize + step;
                while (0..ROWS.len() as isize).contains(&next) {
                    let items = ROWS[next as usize];
                    let preferred = items
                        .get(column)
                        .copied()
                        .filter(|index| self.option_enabled(*index));
                    if let Some(target) = preferred.or_else(|| {
                        items
                            .iter()
                            .copied()
                            .find(|index| self.option_enabled(*index))
                    }) {
                        self.options_focus = target;
                        break;
                    }
                    next += step;
                }
            }
            _ => {}
        }
    }

    fn handle_options(&mut self, input: Input, catalog: &[AppEntry]) -> Action {
        match input {
            Input::Back | Input::OpenOptions => {
                self.screen = Screen::Browser;
                Action::None
            }
            Input::Left | Input::Right | Input::Up | Input::Down => {
                self.move_options_focus(input);
                Action::None
            }
            Input::Confirm if !self.option_enabled(self.options_focus) => Action::None,
            Input::Confirm => match self.options_focus {
                0 => {
                    self.pending_device = 0;
                    Action::None
                }
                1 => {
                    self.pending_device = 1;
                    Action::None
                }
                2 => {
                    self.pending_device = 2;
                    Action::None
                }
                3 => {
                    self.pending_device = 3;
                    Action::None
                }
                4 => {
                    self.pending_sort = SortOrder::Name;
                    Action::None
                }
                5 => {
                    self.pending_sort = SortOrder::ReleaseDate;
                    Action::None
                }
                6 => {
                    self.preferred_device = self.pending_device;
                    self.sort = self.pending_sort;
                    self.filter = match self.pending_sort {
                        SortOrder::Name => Filter::All,
                        SortOrder::ReleaseDate => Filter::MetadataOnly,
                    };
                    self.reconcile_selection(catalog);
                    self.screen = Screen::Browser;
                    Action::ApplyOptions
                }
                7 => {
                    self.screen = Screen::Browser;
                    Action::None
                }
                8 => {
                    self.screen = Screen::Options;
                    Action::LoadNetworkConfig
                }
                _ => Action::None,
            },
            _ => Action::None,
        }
    }

    fn handle_network_settings(&mut self, input: Input) -> Action {
        match input {
            Input::Back | Input::OpenOptions => {
                self.screen = Screen::Options;
                Action::None
            }
            Input::Up => {
                self.network_focus = self.network_focus.saturating_sub(1);
                Action::None
            }
            Input::Down => {
                self.network_focus = (self.network_focus + 1).min(5);
                Action::None
            }
            Input::Left | Input::Right if self.network_focus == 1 => {
                self.network_encryption =
                    cycle_encryption(self.network_encryption, input == Input::Right);
                Action::None
            }
            Input::Confirm => match self.network_focus {
                0 | 2 => {
                    self.keyboard_target_key = self.network_focus == 2;
                    self.keyboard_focus = 0;
                    self.keyboard_symbols = false;
                    self.keyboard_shift = false;
                    self.screen = Screen::NetworkKeyboard;
                    Action::None
                }
                1 => {
                    self.network_encryption = cycle_encryption(self.network_encryption, true);
                    Action::None
                }
                3 => {
                    self.screen = Screen::Options;
                    Action::SaveNetworkConfig
                }
                4 => {
                    self.screen = Screen::Options;
                    Action::None
                }
                5 => Action::ScanNetworks,
                _ => Action::None,
            },
            _ => Action::None,
        }
    }

    fn handle_network_scan(&mut self, input: Input) -> Action {
        match input {
            Input::Back => {
                self.screen = Screen::NetworkSettings;
                Action::None
            }
            Input::Up => {
                self.network_scan_focus = self.network_scan_focus.saturating_sub(1);
                Action::None
            }
            Input::Down => {
                self.network_scan_focus = (self.network_scan_focus + 1)
                    .min(self.scanned_networks.len.saturating_sub(1))
                    .min(7);
                Action::None
            }
            Input::Confirm => {
                if let Some(network) = self
                    .scanned_networks
                    .as_slice()
                    .get(self.network_scan_focus)
                    .copied()
                {
                    self.network_ssid.clear();
                    self.network_ssid.extend_from_slice(network.name_bytes());
                    self.network_key.clear();
                    if let Some(encryption) = network.security.config_encryption() {
                        self.network_encryption = encryption;
                    }
                }
                self.screen = Screen::NetworkSettings;
                Action::None
            }
            _ => Action::None,
        }
    }

    fn handle_network_keyboard(&mut self, input: Input) -> Action {
        match input {
            Input::Back => {
                self.screen = Screen::NetworkSettings;
                Action::None
            }
            Input::Left => {
                self.keyboard_focus = self.keyboard_focus.saturating_sub(1);
                Action::None
            }
            Input::Right => {
                self.keyboard_focus = (self.keyboard_focus + 1).min(NETWORK_KEYBOARD_KEYS - 1);
                Action::None
            }
            Input::Up => {
                self.keyboard_focus = self.keyboard_focus.saturating_sub(NETWORK_KEYBOARD_COLUMNS);
                Action::None
            }
            Input::Down => {
                self.keyboard_focus =
                    (self.keyboard_focus + NETWORK_KEYBOARD_COLUMNS).min(NETWORK_KEYBOARD_KEYS - 1);
                Action::None
            }
            Input::Confirm => {
                if let Some(character) = self.keyboard_char(self.keyboard_focus) {
                    let value = if self.keyboard_target_key {
                        &mut self.network_key
                    } else {
                        &mut self.network_ssid
                    };
                    let max_len = if self.keyboard_target_key { 64 } else { 32 };
                    if value.len() < max_len {
                        value.push(character as u8);
                    }
                } else {
                    match self.keyboard_focus {
                        40 => self.keyboard_symbols = !self.keyboard_symbols,
                        41 => self.keyboard_shift = !self.keyboard_shift,
                        42 => {
                            let value = if self.keyboard_target_key {
                                &mut self.network_key
                            } else {
                                &mut self.network_ssid
                            };
                            let max_len = if self.keyboard_target_key { 64 } else { 32 };
                            if value.len() < max_len {
                                value.push(b' ');
                            }
                        }
                        43 => {
                            if self.keyboard_target_key {
                                self.network_key.pop();
                            } else {
                                self.network_ssid.pop();
                            }
                        }
                        44 => {
                            if self.keyboard_target_key {
                                self.network_key.clear();
                            } else {
                                self.network_ssid.clear();
                            }
                        }
                        45 => self.screen = Screen::NetworkSettings,
                        _ => {}
                    }
                }
                Action::None
            }
            _ => Action::None,
        }
    }

    fn move_selection(&mut self, catalog: &[AppEntry], forward: bool, step: usize) {
        let visible = self.visible_apps(catalog);
        if visible.is_empty() {
            self.selected_index = 0;
            self.selected_app = None;
            return;
        }
        self.selected_index = if forward {
            self.selected_index
                .saturating_add(step)
                .min(visible.len() - 1)
        } else {
            self.selected_index.saturating_sub(step)
        };
        self.selected_app = Some(visible[self.selected_index].directory.clone());
    }

    fn move_page(&mut self, catalog: &[AppEntry], forward: bool) -> Action {
        let visible = self.visible_apps(catalog);
        if visible.is_empty() {
            return Action::None;
        }
        let navigation = self.page_navigation_for_count(visible.len());
        if (forward && !navigation.has_next()) || (!forward && !navigation.has_previous()) {
            return Action::None;
        }
        let capacity = navigation.capacity;
        let current_start = navigation.start;
        let next_start = if forward {
            current_start.saturating_add(capacity)
        } else {
            current_start.saturating_sub(capacity)
        };
        if next_start >= visible.len() {
            return Action::None;
        }
        self.selected_index = next_start;
        self.selected_app = Some(visible[next_start].directory.clone());
        if forward {
            Action::NextPage
        } else {
            Action::PreviousPage
        }
    }

    fn move_menu_focus(&mut self, forward: bool) {
        self.menu_focus = if forward {
            (self.menu_focus + 1).min(self.menu_item_count() - 1)
        } else {
            self.menu_focus.saturating_sub(1)
        };
    }

    fn move_detail_focus(&mut self, forward: bool) {
        self.detail_focus = match (self.detail_focus, forward) {
            (8, false) => 10,
            (9, false) => 8,
            (10, false) => 9,
            (8, true) => 9,
            (9, true) => 10,
            (10, true) => 8,
            _ => 10,
        };
    }

    fn refresh_detail_scroll_limit(&mut self, catalog: &[AppEntry]) {
        if self.screen != Screen::AppDetails
            || self.detail_scroll_app.as_deref() == self.selected_app.as_deref()
        {
            return;
        }
        let description = self
            .selected_app
            .as_deref()
            .and_then(|selected| catalog.iter().find(|app| app.directory == selected))
            .and_then(|app| app.metadata.as_ref())
            .and_then(|metadata| metadata.long_description.as_deref())
            .unwrap_or("No description available.");
        self.detail_description_max_scroll =
            crate::text_layout::wrap_lines(description, DETAIL_DESCRIPTION_LINE_WIDTH)
                .len()
                .saturating_sub(DETAIL_DESCRIPTION_VISIBLE_LINES);
        self.detail_description_scroll = self
            .detail_description_scroll
            .min(self.detail_description_max_scroll);
        self.detail_scroll_app = self.selected_app.clone();
    }

    fn reconcile_selection(&mut self, catalog: &[AppEntry]) {
        let visible = self.visible_apps(catalog);
        if visible.is_empty() {
            self.selected_index = 0;
            self.selected_app = None;
            return;
        }
        if let Some(index) = self
            .selected_app
            .as_deref()
            .and_then(|identity| visible.iter().position(|entry| entry.directory == identity))
        {
            self.selected_index = index;
        } else {
            // HBC focuses the first entry when the selected app is not in the
            // current filtered view or has disappeared from the catalog.
            self.selected_index = 0;
            self.selected_app = Some(visible[self.selected_index].directory.clone());
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkConfigEditError {
    MissingSnapshot,
    Profile(crate::network_config::NetworkProfileError),
    Wifi(crate::network_config::WifiValidationError),
}

fn cycle_encryption(current: u8, forward: bool) -> u8 {
    const MODES: [u8; 6] = [0, 1, 2, 4, 5, 6];
    let index = MODES.iter().position(|mode| *mode == current).unwrap_or(0);
    let next = if forward {
        (index + 1) % MODES.len()
    } else {
        (index + MODES.len() - 1) % MODES.len()
    };
    MODES[next]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AppKind;
    use crate::metadata::AppMetadata;

    fn app(directory: &str, name: &str, icon: bool) -> AppEntry {
        AppEntry {
            directory: directory.into(),
            kind: AppKind::Elf,
            size: 1,
            has_icon: icon,
            metadata: Some(AppMetadata {
                name: Some(name.into()),
                ..AppMetadata::default()
            }),
        }
    }

    #[test]
    fn saving_unchanged_network_settings_preserves_non_ascii_credentials() {
        let mut config = crate::network_config::NetworkConfig::from_bytes(
            [0; crate::network_config::CONFIG_LEN],
        );
        {
            let mut profile = config.profile_mut(0).unwrap();
            profile.set_flag(crate::network_config::IS_ACTIVE, true);
            let mut wifi = profile.wifi_mut();
            assert!(wifi.set_ssid(&[0x57, 0xc3, 0xa9]));
            assert!(wifi.set_encryption(5));
            assert!(wifi.set_key(&[0xc3, 0xa9, b'1', b'2', b'3', b'4', b'5', b'6']));
        }
        let mut controller = AppController::new();
        controller.set_network_config(config.clone());
        assert_eq!(controller.network_config_for_save(), Ok(config));
    }

    #[test]
    fn about_credits_scroll_after_delay_and_clamp_at_end() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        for _ in 0..120 {
            controller.tick_about_scroll();
        }
        assert_eq!(controller.about_scroll(), 0);
        for _ in 0..3 {
            controller.tick_about_scroll();
        }
        assert_eq!(controller.about_scroll(), 1);
        for _ in 0..10_000 {
            controller.tick_about_scroll();
        }
        let max = crate::text_layout::wrap_memo_lines(ABOUT_CREDITS, ABOUT_MEMO_LINE_WIDTH)
            .len()
            .saturating_sub(ABOUT_MEMO_VISIBLE_LINES);
        assert_eq!(controller.about_scroll(), max);
    }

    #[test]
    fn about_scroll_includes_one_active_theme_credit_line() {
        let mut controller = AppController::new();
        controller.screen = Screen::About;
        let base_max = controller.about_max_scroll();
        controller.set_about_theme_description(true);
        for _ in 0..10_000 {
            controller.tick_about_scroll();
        }
        assert_eq!(controller.about_scroll(), base_max + 1);
        controller.set_about_theme_description(false);
        assert_eq!(controller.about_scroll(), base_max);
    }

    #[test]
    fn typed_notices_preserve_kind_and_generic_notices_remain_generic() {
        let mut controller = AppController::new();
        controller.show_notice_with_kind("Storage is full.".into(), NoticeKind::Warning);
        assert_eq!(controller.notice(), Some("Storage is full."));
        assert_eq!(controller.notice_kind(), NoticeKind::Warning);
        controller.show_notice("Done.".into());
        assert_eq!(controller.notice_kind(), NoticeKind::Generic);
    }

    #[test]
    fn browser_navigation_and_detail_back_keep_identity() {
        let catalog = [app("one", "One", true), app("two", "Two", true)];
        let mut controller = AppController::new();
        assert_eq!(controller.selected_app.as_deref(), None);
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 10);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::Browser);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.handle(Input::Left, &catalog), Action::None);
        assert_eq!(controller.detail_focus(), 9);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::LaunchSelected
        );
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.detail_focus(), 8);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        controller.handle(Input::Back, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 8);
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
    }

    #[test]
    fn detail_up_and_down_scroll_the_description_without_moving_focus() {
        let mut controller = AppController::new();
        let mut entry = app("a", "A", false);
        entry.metadata.as_mut().unwrap().long_description = Some("A ".repeat(1_000));
        let catalog = vec![entry];
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.detail_description_scroll(), 0);

        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.detail_description_scroll(), 2);
        assert_eq!(controller.detail_focus(), 10);

        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Up, &catalog);
        assert_eq!(controller.detail_description_scroll(), 0);
        assert_eq!(controller.detail_focus(), 10);

        for _ in 0..100 {
            controller.handle(Input::Down, &catalog);
        }
        assert!(controller.detail_description_scroll() > 0);
        let max_scroll = controller.detail_description_max_scroll;
        assert_eq!(controller.detail_description_scroll(), max_scroll);
        controller.handle(Input::Up, &catalog);
        assert_eq!(controller.detail_description_scroll(), max_scroll - 1);
    }

    #[test]
    fn detail_stick_scroll_accumulates_pixels_into_lines() {
        let mut controller = AppController::new();
        let mut entry = app("a", "A", false);
        entry.metadata.as_mut().unwrap().long_description = Some("A ".repeat(1_000));
        let catalog = vec![entry];
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.scroll_detail_by_stick(8);
        assert_eq!(controller.detail_description_scroll(), 1);
        controller.scroll_detail_by_stick(1);
        assert_eq!(controller.detail_description_scroll(), 0);
        controller.scroll_detail_by_stick(-18);
        assert_eq!(controller.detail_description_scroll(), 2);
    }

    #[test]
    fn dragging_detail_memo_scrolls_by_pointer_delta_and_stops_on_release() {
        let mut controller = AppController::new();
        let mut entry = app("a", "A", false);
        entry.metadata.as_mut().unwrap().long_description = Some("A ".repeat(1_000));
        let catalog = vec![entry];
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);

        controller.handle_pointer_frame(
            Some(PointerTarget::DetailMemo),
            Some(200),
            true,
            false,
            &catalog,
        );
        controller.handle_pointer_frame(None, Some(-88), false, true, &catalog);
        assert_eq!(controller.detail_description_scroll(), 1);

        controller.handle_pointer_frame(
            Some(PointerTarget::DetailMemo),
            Some(200),
            true,
            false,
            &catalog,
        );
        controller.handle_pointer_frame(None, None, false, true, &catalog);
        controller.handle_pointer_frame(None, Some(-88), false, true, &catalog);
        assert_eq!(controller.detail_description_scroll(), 1);

        controller.handle_pointer_frame(None, None, false, false, &catalog);
        assert_eq!(controller.detail_description_scroll(), 1);
    }

    #[test]
    fn dragging_long_notice_memo_scrolls_and_clamps_to_its_text() {
        let mut controller = AppController::new();
        controller.show_notice(
            (0..400)
                .map(|n| format!("notice{n}"))
                .collect::<Vec<_>>()
                .join(" "),
        );
        assert_eq!(controller.notice_scroll(), 0);
        controller.handle_pointer_frame(
            Some(PointerTarget::NoticeMemo),
            Some(220),
            true,
            true,
            &[],
        );
        controller.handle_pointer_frame(
            Some(PointerTarget::NoticeMemo),
            Some(-200),
            false,
            true,
            &[],
        );
        assert!(controller.notice_scroll() > 0);
        let max_scroll = controller.notice_scroll_max;
        controller.handle_pointer_frame(
            Some(PointerTarget::NoticeMemo),
            Some(i16::MIN),
            false,
            true,
            &[],
        );
        assert_eq!(controller.notice_scroll(), max_scroll);
        controller.handle_pointer_frame(None, None, false, false, &[]);
        controller.scroll_active_memo_by_stick(900);
        assert_eq!(controller.notice_scroll(), max_scroll);
    }

    #[test]
    fn pointer_selects_and_activates_the_same_items_as_confirm() {
        let catalog = [app("one", "One", false), app("two", "Two", false)];
        let mut controller = AppController::new();
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::App(1)), false, &catalog),
            Action::None
        );
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::App(1)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
    }

    #[test]
    fn pointer_app_indexes_follow_the_filtered_visible_list() {
        let mut hidden = app("hidden", "Hidden", false);
        hidden.metadata = None;
        let catalog = [hidden, app("visible", "Visible", false)];
        let mut controller = AppController::new();
        controller.filter = crate::catalog::Filter::MetadataOnly;
        controller.sort = SortOrder::ReleaseDate;
        controller.handle_pointer(Some(PointerTarget::App(0)), false, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("visible"));
    }

    #[test]
    fn pointer_focuses_modal_buttons_and_uses_normal_actions() {
        let catalog = [app("one", "One", false)];
        let mut controller = AppController::new();
        controller.handle(Input::OpenOptions, &catalog);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Option(4)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.pending_sort(), SortOrder::Name);
        assert_eq!(controller.options_focus(), 4);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Option(6)), true, &catalog),
            Action::ApplyOptions
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn pointer_activates_menu_details_delete_notice_and_page_targets() {
        let catalog = [
            app("one", "One", false),
            app("two", "Two", false),
            app("three", "Three", false),
            app("four", "Four", false),
            app("five", "Five", false),
            app("six", "Six", false),
        ];
        let mut controller = AppController::new();

        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Page { forward: true }), true, &catalog),
            Action::NextPage
        );
        assert_eq!(controller.selected_app.as_deref(), Some("two"));
        controller.handle(Input::Home, &catalog);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Menu(1)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::About);
        controller.handle(Input::Back, &catalog);
        controller.handle_pointer(Some(PointerTarget::Menu(0)), true, &catalog);
        controller.handle_pointer(Some(PointerTarget::App(5)), true, &catalog);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::Detail(8)), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        assert_eq!(
            controller.handle_pointer(
                Some(PointerTarget::DeleteChoice { yes: true }),
                true,
                &catalog
            ),
            Action::DeleteSelected
        );
        controller.show_notice("Issue".into());
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::NoticeConfirm), true, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn deleting_an_app_requires_an_explicit_yes_confirmation() {
        let catalog = [app("one", "One", true)];
        let mut controller = AppController::new();
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Left, &catalog);
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.detail_focus(), 8);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 8);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.screen, Screen::DeleteConfirm);
        controller.handle(Input::Back, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.detail_focus(), 8);

        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Right, &catalog);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::DeleteSelected
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn received_zip_requires_yes_and_no_or_back_discards_it() {
        let mut controller = AppController::new();
        controller.request_wiiload_zip_confirmation();
        assert_eq!(controller.screen, Screen::WiiloadConfirm);
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::RejectWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        controller.handle(Input::Right, &[]);
        assert_eq!(controller.wiiload_confirmation_yes(), true);
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::AcceptWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle(Input::Back, &[]),
            Action::RejectWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle(Input::Home, &[]),
            Action::RejectWiiloadZip
        );
        assert_eq!(controller.screen, Screen::MainMenu);
    }

    #[test]
    fn long_received_zip_confirmation_scrolls_without_changing_yes_no_actions() {
        let mut controller = AppController::new();
        controller.request_wiiload_zip_confirmation();
        controller.set_wiiload_confirmation_text("ZIP details ".repeat(240));
        let max_scroll = controller.wiiload_confirmation_max_scroll();
        assert!(max_scroll > 0);
        assert_eq!(controller.wiiload_confirmation_scroll(), 0);

        assert_eq!(controller.handle(Input::Down, &[]), Action::None);
        assert_eq!(controller.wiiload_confirmation_scroll(), 1);
        assert_eq!(controller.handle(Input::Up, &[]), Action::None);
        assert_eq!(controller.wiiload_confirmation_scroll(), 0);
        for _ in 0..max_scroll.saturating_add(2) {
            controller.handle(Input::Down, &[]);
        }
        assert_eq!(controller.wiiload_confirmation_scroll(), max_scroll);

        assert_eq!(controller.handle(Input::Right, &[]), Action::None);
        assert!(controller.wiiload_confirmation_yes());
        assert_eq!(controller.handle(Input::Left, &[]), Action::None);
        assert!(!controller.wiiload_confirmation_yes());
        assert_eq!(controller.handle(Input::Right, &[]), Action::None);
        assert!(controller.wiiload_confirmation_yes());
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::AcceptWiiloadZip
        );
    }

    #[test]
    fn notice_and_received_zip_memos_keep_separate_pixel_remainders() {
        let mut controller = AppController::new();
        controller.show_notice("Long notice ".repeat(120));
        controller.scroll_active_memo_by_stick(8);
        assert_eq!(controller.notice_scroll(), 0);
        assert_eq!(controller.notice_scroll_pixel_remainder, 8);

        controller.request_wiiload_zip_confirmation();
        controller.set_wiiload_confirmation_text("Long ZIP details ".repeat(120));
        controller.scroll_active_memo_by_stick(1);
        assert_eq!(controller.wiiload_confirmation_scroll(), 0);
        assert_eq!(controller.wiiload_confirmation_scroll_pixel_remainder, 1);
        assert_eq!(controller.notice_scroll_pixel_remainder, 8);

        controller.scroll_active_memo_by_stick(8);
        assert_eq!(controller.wiiload_confirmation_scroll(), 1);
        assert_eq!(controller.wiiload_confirmation_scroll_pixel_remainder, 0);
        assert_eq!(controller.notice_scroll_pixel_remainder, 8);
    }

    #[test]
    fn received_zip_pointer_choices_use_the_same_yes_no_actions() {
        let mut controller = AppController::new();
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::WiiloadChoice { yes: true }), true, &[]),
            Action::AcceptWiiloadZip
        );
        controller.request_wiiload_zip_confirmation();
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::WiiloadChoice { yes: false }), true, &[]),
            Action::RejectWiiloadZip
        );
    }

    #[test]
    fn received_zip_loading_progress_is_bounded_and_only_changes_during_install() {
        let mut controller = AppController::new();
        controller.set_wiiload_install_progress(99);
        assert_eq!(controller.wiiload_install_progress(), 0);
        controller.show_wiiload_installing();
        controller.set_wiiload_install_progress(255);
        assert_eq!(controller.wiiload_install_progress(), 100);
        controller.finish_wiiload_installing();
        controller.set_wiiload_install_progress(22);
        assert_eq!(controller.wiiload_install_progress(), 100);
    }

    #[test]
    fn wiiload_receive_progress_is_bounded_and_receiving_screen_can_finish() {
        let mut controller = AppController::new();
        controller.show_wiiload_receiving();
        controller.set_wiiload_install_progress(35);
        assert_eq!(controller.screen, Screen::WiiloadReceiving);
        assert_eq!(controller.wiiload_install_progress(), 35);
        controller.set_wiiload_install_progress(255);
        assert_eq!(controller.wiiload_install_progress(), 100);
        controller.finish_wiiload_receiving();
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn zip_install_screen_ignores_controller_and_pointer_input() {
        let mut controller = AppController::new();
        controller.show_wiiload_installing();
        assert_eq!(controller.screen, Screen::WiiloadInstalling);
        assert_eq!(controller.handle(Input::Home, &[]), Action::None);
        assert_eq!(
            controller.handle_pointer(Some(PointerTarget::NoticeConfirm), true, &[]),
            Action::None
        );
        controller.finish_wiiload_installing();
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn theme_detail_action_applies_the_theme_instead_of_launching_it() {
        let mut theme = app("ocean", "Ocean", true);
        theme.kind = AppKind::Theme;
        let catalog = [theme];
        let mut controller = AppController::new();
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.screen, Screen::AppDetails);
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.detail_focus(), 9);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::ApplyTheme
        );
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn browse_mode_toggle_keeps_selected_app_identity() {
        let catalog = [app("with-icon", "A", true), app("without-icon", "B", false)];
        let mut controller = AppController::new();
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("without-icon"));
        controller.handle(Input::ToggleBrowseMode, &catalog);
        assert_eq!(controller.browser_layout().columns, 3);
        assert_eq!(controller.selected_app.as_deref(), Some("without-icon"));
        assert_eq!(controller.selected_index(), 1);
        controller.handle(Input::ToggleBrowseMode, &catalog);
        assert_eq!(controller.browser_layout().columns, 1);
    }

    #[test]
    fn empty_catalog_clears_selection_and_confirmation_does_nothing() {
        let mut controller = AppController::new();
        controller.selected_app = Some("stale-entry".into());
        assert_eq!(controller.handle(Input::Down, &[]), Action::None);
        assert_eq!(controller.selected_app, None);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::None);
        assert_eq!(controller.screen, Screen::Browser);
    }

    #[test]
    fn home_menu_navigation_matches_main_view_actions() {
        let mut controller = AppController::new();
        controller.set_bootmii_available(true);
        controller.handle(Input::Home, &[]);
        assert_eq!(controller.screen, Screen::MainMenu);
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::None);
        assert_eq!(controller.screen, Screen::About);
        controller.handle(Input::Back, &[]);
        assert_eq!(controller.screen, Screen::MainMenu);
        controller.handle(Input::Down, &[]);
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::LaunchBootMii
        );
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::Exit);
    }

    #[test]
    fn options_changes_are_pending_until_ok_and_back_discards_them() {
        let catalog = [app("one", "One", true)];
        let mut controller = AppController::new();
        controller.set_preferred_device(1);
        controller.sort = SortOrder::ReleaseDate;

        assert_eq!(
            controller.handle(Input::OpenOptions, &catalog),
            Action::None
        );
        assert_eq!(controller.screen, Screen::Options);
        assert_eq!(controller.pending_device(), 1);
        assert_eq!(controller.pending_sort(), SortOrder::ReleaseDate);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Left, &catalog);
        assert_eq!(controller.options_focus(), 4);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.pending_sort(), SortOrder::Name);
        assert_eq!(controller.handle(Input::Back, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::Browser);
        assert_eq!(controller.preferred_device(), 1);
        assert_eq!(controller.sort, SortOrder::ReleaseDate);

        controller.handle(Input::OpenOptions, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Left, &catalog);
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.options_focus(), 6);
        assert_eq!(
            controller.handle(Input::Confirm, &catalog),
            Action::ApplyOptions
        );
        assert_eq!(controller.screen, Screen::Browser);
        assert_eq!(controller.preferred_device(), 1);
        assert_eq!(controller.sort, SortOrder::Name);
        assert_eq!(controller.filter, Filter::All);
    }

    #[test]
    fn options_starts_with_the_active_device_after_preference_fallback() {
        let mut controller = AppController::new();
        controller.set_preferred_device(1);
        controller.set_active_device(Some(0));
        controller.handle(Input::OpenOptions, &[]);
        assert_eq!(controller.pending_device(), 0);
    }

    #[test]
    fn main_menu_hides_and_remaps_bootmii_when_ios_is_unavailable() {
        let mut controller = AppController::new();
        controller.handle(Input::Home, &[]);
        assert_eq!(controller.menu_item_count(), 4);
        controller.handle(Input::Down, &[]);
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::Exit);
        controller.handle(Input::Down, &[]);
        assert_eq!(controller.handle(Input::Confirm, &[]), Action::Shutdown);

        let mut available = AppController::new();
        available.set_bootmii_available(true);
        available.handle(Input::Home, &[]);
        available.handle(Input::Down, &[]);
        available.handle(Input::Down, &[]);
        assert_eq!(available.menu_item_count(), 5);
        assert_eq!(available.handle(Input::Confirm, &[]), Action::LaunchBootMii);
    }

    #[test]
    fn notice_dismissal_restores_the_screen_that_opened_it() {
        let catalog = [app("one", "One", true)];
        let mut controller = AppController::new();
        controller.handle(Input::Confirm, &catalog);
        controller.show_notice("Load failed".into());
        assert_eq!(controller.screen, Screen::Notice);
        assert_eq!(controller.notice(), Some("Load failed"));
        assert_eq!(controller.handle(Input::Confirm, &catalog), Action::None);
        assert_eq!(controller.screen, Screen::AppDetails);
        assert_eq!(controller.notice(), None);
    }

    #[test]
    fn replacing_a_notice_keeps_the_original_return_screen() {
        let catalog = [app("one", "One", true)];
        for open_details in [false, true] {
            for dismiss in [Input::Confirm, Input::Back, Input::Home] {
                let mut controller = AppController::new();
                if open_details {
                    controller.handle(Input::Confirm, &catalog);
                }
                let original_screen = controller.screen;
                controller.show_notice("Playtime reset failed".into());
                controller.show_notice("No SD or USB storage device is available.".into());
                controller.show_notice("No SD or USB storage device is available.".into());
                assert_eq!(controller.screen, Screen::Notice);
                assert_eq!(
                    controller.notice(),
                    Some("No SD or USB storage device is available.")
                );
                assert_eq!(controller.handle(dismiss, &catalog), Action::None);
                assert_eq!(controller.screen, original_screen);
                assert_eq!(controller.notice(), None);
            }
        }
    }

    #[test]
    fn page_inputs_move_selection_and_stop_at_page_boundaries() {
        let catalog = [
            app("a", "A", true),
            app("b", "B", true),
            app("c", "C", true),
            app("d", "D", true),
            app("e", "E", true),
            app("f", "F", true),
            app("g", "G", true),
        ];
        let mut controller = AppController::new();
        assert_eq!(
            controller.handle(Input::NextPage, &catalog),
            Action::NextPage
        );
        assert_eq!(controller.selected_index(), 5);
        assert_eq!(controller.selected_app.as_deref(), Some("f"));
        assert_eq!(controller.page_start(), 5);
        assert_eq!(controller.visible_page(&catalog).len(), 2);
        assert_eq!(controller.handle(Input::NextPage, &catalog), Action::None);
        assert_eq!(
            controller.handle(Input::PreviousPage, &catalog),
            Action::PreviousPage
        );
        assert_eq!(controller.selected_index(), 0);
        assert_eq!(controller.selected_app.as_deref(), Some("a"));
        assert_eq!(
            controller.handle(Input::PreviousPage, &catalog),
            Action::None
        );
    }

    #[test]
    fn grid_controls_move_by_row_and_keep_identity_after_resort() {
        let catalog = [
            app("a", "A", true),
            app("b", "B", true),
            app("c", "C", true),
            app("d", "D", true),
            app("e", "E", true),
            app("f", "F", true),
        ];
        let mut controller = AppController::new();
        controller.set_browser_layout(
            BrowserLayout {
                columns: 3,
                rows: 2,
            },
            &catalog,
        );
        controller.handle(Input::Right, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("b"));
        controller.handle(Input::Down, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("e"));
        controller.sort = SortOrder::ReleaseDate;
        controller.filter = Filter::MetadataOnly;
        controller.handle(Input::OpenOptions, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Up, &catalog);
        controller.handle(Input::Left, &catalog);
        controller.handle(Input::Confirm, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Confirm, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("e"));
        assert_eq!(controller.selected_index(), 4);
        assert_eq!(controller.sort, SortOrder::Name);
    }

    #[test]
    fn missing_selection_uses_first_visible_app() {
        let catalog = [
            app("a", "A", true),
            app("b", "B", true),
            app("c", "C", true),
        ];
        let mut controller = AppController::new();
        controller.handle(Input::Down, &catalog);
        controller.handle(Input::Down, &catalog);
        let catalog = [app("a", "A", true), app("c", "C", true)];
        controller.handle(Input::Up, &catalog);
        assert_eq!(controller.selected_app.as_deref(), Some("a"));
        assert_eq!(controller.selected_index(), 0);
    }

    #[test]
    fn options_skip_unavailable_devices_and_network_is_controller_selectable() {
        let mut controller = AppController::new();
        controller.set_device_availability([false; 4]);
        controller.handle(Input::OpenOptions, &[]);
        assert_eq!(controller.options_focus(), 7);
        controller.handle(Input::Up, &[]);
        assert_eq!(controller.options_focus(), 8);
        assert_eq!(
            controller.handle(Input::Confirm, &[]),
            Action::LoadNetworkConfig
        );
        controller.handle(Input::Up, &[]);
        assert_eq!(controller.options_focus(), 4);
        controller.handle(Input::Up, &[]);
        assert_eq!(controller.options_focus(), 4);
        let pending = controller.pending_device();
        controller.handle_pointer(Some(PointerTarget::Option(1)), true, &[]);
        assert_eq!(controller.options_focus(), 4);
        assert_eq!(controller.pending_device(), pending);
    }

    #[test]
    fn options_can_select_detected_devices_and_reject_removed_devices() {
        let mut controller = AppController::new();
        controller.set_device_availability([false, false, true, true]);
        controller.handle(Input::OpenOptions, &[]);
        controller.handle(Input::Up, &[]);
        controller.handle(Input::Up, &[]);
        controller.handle(Input::Up, &[]);
        assert_eq!(controller.options_focus(), 2);
        controller.handle(Input::Confirm, &[]);
        assert_eq!(controller.pending_device(), 2);
        controller.handle(Input::Right, &[]);
        controller.handle(Input::Confirm, &[]);
        assert_eq!(controller.pending_device(), 3);
        controller.set_device_availability([false; 4]);
        controller.pending_device = 2;
        controller.handle(Input::Confirm, &[]);
        assert_eq!(controller.pending_device(), 2);
    }
}
