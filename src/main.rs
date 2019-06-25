// extern crate termion;
// use termion::{ color,
//     style,
//     raw::IntoRawMode,
//     cursor::{
//         DetectCursorPos,
//     },
//     input::{
//         MouseTerminal,
//         TermRead,
//         Events,
//     },
//     event::{
//         MouseEvent,
//         Event,
//         Key,
//     },
// };

extern crate crossterm;

use crossterm::{
    Screen,
    Crossterm,
    TerminalCursor,
    TerminalInput,
    style::ObjectStyle,
    Color,
    Attribute,
    cursor,
};

// use std::io::Read;
use std::io::Write;
// use std::io;
use std::borrow::Cow;
use std::vec::Vec;
use std::fs::File;
use std::path::{
    Path,
    PathBuf,
};
use std::string::String;
use std::fmt;
use std::collections::HashMap;

// == TYPES ==

#[derive(Clone, Debug)]
struct Rect{
    x: u16, y: u16,
    w: u16, h: u16,
}

type Result = std::result::Result<(), String>;

struct RootWin {
    term: Crossterm,
    screen: Screen,

    geo: Rect,
    draw_area: Option<Rect>,
}

enum SortOrder {
    Name,
}

#[derive(Clone)]
struct DirView {
    geo: Rect,
    dir: PathBuf,
    sel: Vec<usize>, // list of delected indices
    scroll: usize,
    entries: Vec<PathBuf>, // list of entries in the original order
    sorted_indices: Vec<usize>, // indices of entries in sorted order
}

struct FileView {
    geo: Rect,
    path: PathBuf,
    file: File,
    scroll: usize,
}

enum View {
    Dir(DirView),
    File(FileView),
}

// TODO: Find better names for these actions
enum Action {
    Quit,
    MoveDown(usize),
    MoveUp(usize),
    MoveLeft(usize),
    MoveRight(usize),
}

type ViewMap<'a> = HashMap<PathBuf, View>;
type ColorMap<'a> = HashMap<&'a str, ObjectStyle>;
type KeyBinds = HashMap<char, Action>;

struct Nv<'a> {
    root: RootWin,

    views: ViewMap<'a>,
    cur_path: PathBuf,

    views_shown: usize,

    colors: ColorMap<'a>,
    binds: KeyBinds,
}

// == TRAITS ==

trait Canvas {
    fn goto(&mut self, x: u16, y: u16);
    fn print(&mut self, s: impl fmt::Display);
}

trait Drawable<Data> {
    fn get_geo(&self) -> Rect;
    fn draw(&mut self, d: &mut impl Canvas, data: Data);
}

// == IMPLS ==

impl Rect {
    fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self {x, y, w, h}
    }
}

impl RootWin {
    fn new(geo: Rect) -> Self {
        let screen = Screen::new(true);
        let term = Crossterm::from_screen(&screen);

        Self {
            // stdout: MouseTerminal::from(std::io::stdout().into_raw_mode().unwrap()),
            term: term,
            screen: screen,


            geo: geo,
            draw_area: None,
        }
    }

    fn get_fullscreen_geo(&self) -> Rect {
        let mut r = self.geo.clone();
        r.x = 0;
        r.y = 0;
        r
    }

    // for now only ensures height
    fn ensure_geo(&mut self) -> (u16, u16) {
        let term = self.term.terminal();

        let mut y_ofs = 0;
        let wdim = term.terminal_size();

        if self.geo.w + self.geo.x > wdim.0 {
            self.geo.w = wdim.0 - self.geo.x;
        }

        let hdif = (wdim.1 -1) - self.geo.y;

        if hdif < self.geo.h {
            y_ofs = self.geo.h - hdif -1;
            term.scroll_up(y_ofs as i16).unwrap();
        }

        self.geo.y -= y_ofs;
        (0, y_ofs)
    }

    fn abs_cursor_pos(&mut self) -> (u16, u16) {
        // self.term.cursor().pos()
        cursor::from_screen(&self.screen).pos()
    }

    fn goto_abs(&mut self, x: u16, y: u16) {
        // self.term.cursor().goto(x, y).unwrap();
        cursor::from_screen(&self.screen).goto(x, y).unwrap();
    }

    fn input<'a>(&'a self) -> TerminalInput<'a> {
        crossterm::input::from_screen(&self.screen)
        // self.term.input()
    }

    fn cursor<'a>(&'a self) -> TerminalCursor<'a> {
        cursor::from_screen(&self.screen)
    }

    fn clear(&mut self) {
        self.draw_area = Some(self.geo.clone());
        self.draw_area.as_mut().unwrap().x = 0;
        self.draw_area.as_mut().unwrap().y = 0;

        for y in 0..self.geo.h {
            self.goto(0, y);
            self.print(&format!("{: <1$}", "", self.geo.w as usize))
        }
    }

    fn draw<Data>(&mut self, d : &mut impl Drawable<Data>, data: Data) {
        self.draw_area = Some(d.get_geo());
        d.draw(self, data);
        self.draw_area = None;
    }

    fn abs_pos(&self, mut x: u16, mut y: u16) -> (u16, u16) {
        let da = self.draw_area.as_ref().unwrap();

        if x >= da.w {
            panic!("Out of bounds coord!");
        }
        if y >= da.h {
            panic!("Out of bounds coord!");
        }

        x += da.x + self.geo.x;
        y += da.y + self.geo.y;

        if x >= self.geo.w + self.geo.x {
            panic!("Out of bounds abs coord! x: {}", x);
        }
        if y >= self.geo.h + self.geo.y {
            panic!("Out of bounds abs coord! y: {} {}", y, self.geo.y);
        }

        return (x, y);
    }
}

impl Canvas for RootWin {
    fn goto(&mut self, x: u16, y: u16) {
        let (x, y) = self.abs_pos(x, y);

        self.goto_abs(x, y);
    }

    fn print(&mut self, s: impl fmt::Display) {
        // NOTE: This first flush is probably not necessary
        self.screen.flush().unwrap();

        // For some reason, we have to flush even though we're in raw mode
        write!(self.screen, "{}", s).unwrap();
        self.screen.flush().unwrap();
    }
}

impl DirView {
    fn new<P: AsRef<Path>>(geo: Rect, dir: P) -> Self {
        Self {
            geo: geo,
            dir: std::fs::canonicalize(dir).unwrap(),
            sel: vec![0],
            scroll: 0,

            entries: vec![],
            sorted_indices: vec![],
        }
    }

    fn scan_dir(&mut self) {
        self.entries = self.dir
            .read_dir()
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();

        if self.entries.len() <= *self.sel.iter().max().unwrap_or(&0) {
            self.sel = vec![0];
        }

        self.sorted_indices = (0..self.entries.len()).collect();
    }

    fn sort(&mut self, by: SortOrder) {
        // Temporarily restore the selections to absolute indices
        for sel in self.sel.iter_mut() { 
            *sel = self.sorted_indices[*sel];
        }

        match by {
            SortOrder::Name => {
                let entries = &self.entries;
                self.sorted_indices
                    .sort_by(|a, b| {
                        let aname = entries[*a].file_name();
                        let bname = entries[*b].file_name();
                        aname.cmp(&bname)
                    });
            },
        }

        // Restore the selections to sorted indices
        for sel in self.sel.iter_mut() { 
            // This unwrap should be safe as we only reordered the indices
            *sel = self.sorted_indices.iter().position(|&i| i == *sel).unwrap();
        }
    }

    fn inc_sel(&mut self, ofs: isize) -> isize {
        use std::cmp::{min, max};

        let max_sel   = self.entries.len() -1;
        let old_index = self.sel[0];

        self.sel[0] = min(max(0, self.sel[0] as isize + ofs) as usize, max_sel);

        return self.sel[0] as isize - old_index as isize;
    }

    fn ensure_sel_in_view(&mut self) {
        let sel_y: isize = 
            self.sel[0] as isize -
            self.scroll as isize;

        if sel_y >= self.geo.h as isize {
            self.scroll += (sel_y - (self.geo.h as isize) + 1) as usize;

        } else if sel_y < 0 {
            self.scroll -= -sel_y as usize;
        }
    }

    fn make_selected_dir_view(&self) -> Option<Self> {
        let dir = &self.sel_path();

        if dir.is_dir() {
            Some(Self::new(self.geo.clone(), dir.clone()))
        } else {
            None
        }
    }

    fn make_selected_file_view(&self) -> Option<FileView> {
        let file = &self.sel_path();

        if file.is_file() {
            Some(FileView::new(self.geo.clone(), file.clone()))
        } else {
            None
        }
    }

    fn make_selected_view(&self) -> Option<View> {
        if self.sel_path().is_dir() {
            self.make_selected_dir_view().map(|dv| dv.into())
        } else {
            self.make_selected_file_view().map(|fv| fv.into())
        }
    }

    fn make_parent_dir_view(&self) -> Option<Self> {
        self.dir.parent().map(|dir|{
            Self::new(self.geo.clone(), dir.to_owned())
        })
    }

    fn select_first(&mut self) {
        if  self.sel.len() > 0 &&
            self.sorted_indices.len() > 0 {

            self.sel[0] = 0;
        }
    }

    fn select_by_name(&mut self, name: String) {
        if self.sel.len() > 0 {
            let sorted_i = self.sorted_indices.iter()
                               .position(|&abs_i| {
                                   self.entries[abs_i].file_name().unwrap()
                                                      .to_str().unwrap()
                                                      .to_owned() 
                                                      == name
                               });
            if let Some(sorted_i) = sorted_i {
                self.sel[0] = sorted_i;
            }
        }
    }

    fn dir_path(&self) -> &Path {
        &self.dir
    }

    fn dir_file_name(&self) -> String {
        self.dir.file_name().unwrap()
                .to_str().unwrap()
                .to_owned()
    }

    fn sel_path(&self) -> &Path {
        self.entry_path(self.sorted_indices[self.sel[0]])
    }

    fn sel_file_name(&self) -> String {
        self.entry_file_name(self.sorted_indices[self.sel[0]])
    }

    fn entry_path(&self, i: usize) -> &Path {
        &self.entries[i]
    }

    fn entry_file_name(&self, i: usize) -> String { 
        self.entry_path(i)
            .file_name().unwrap()
            .to_str().unwrap()
            .to_owned()
    }

    fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

trait StrUtils {
    fn ellipsize(&mut self, len: usize);
}

impl StrUtils for String {
    fn ellipsize(&mut self, len: usize) {
        if self.len() > len {
            self.truncate(len - 1);
            self.push('…');
        }
    }
}

impl<'a> Drawable<&ColorMap<'a>> for DirView {
    fn get_geo(&self) -> Rect {
        self.geo.clone()
    }

    fn draw(&mut self, d: &mut impl Canvas, c: &ColorMap) {
        if self.dir.is_dir() {

            let dir_iiter = self.sorted_indices.iter().skip(self.scroll);

            for (i, de) in (0..self.geo.h).zip(dir_iiter) {

                d.goto(0, i);

                // Plain item
                let mut fname = self.entry_file_name(*de);
                fname.ellipsize(self.geo.w as usize);

                let w = self.geo.w as usize;
                let p = format!("{0: <1$}", &fname, w);

                // Apply Styles
                let s = match self.entries[*de].is_file() {
                    true  => "File",
                    false => "Directory",
                };
                let p = c[s].apply_to(p);

                if self.sel.contains(&(self.scroll + i as usize)) {
                    let p = c["Selected"].apply_to(p);
                    d.print(p);
                } else {
                    d.print(p);
                }

            }
        }
    }
}

impl FileView {
    fn new<P: AsRef<Path>>(geo: Rect, path: P) -> Self {
        let mut file = File::open(path).unwrap();
        let buffer = file.by_ref().take()

        Self {
            geo: geo,
            path: path.as_ref().to_owned(),
            file: file,
            buffer: buffer,
            scroll: 0,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn file_name(&self) -> String {
        self.path.file_name().unwrap()
                 .to_str().unwrap()
                 .to_owned()
    }

    fn make_parent_dir_view(&self) -> Option<DirView> {
        self.path.parent().map(|dir|{
            DirView::new(self.geo.clone(), dir.to_owned())
        })
    }
}

impl<'a> Drawable<&ColorMap<'a>> for FileView {
    fn get_geo(&self) -> Rect {
        self.geo.clone()
    }

    fn draw(&mut self, d: &mut impl Canvas, c: &ColorMap) {

    }
}

impl View {
    fn make_parent_dir_view(&self) -> Option<DirView> {
        match self {
            View::Dir(ref dv) => dv.make_parent_dir_view(),
            View::File(ref fv) => fv.make_parent_dir_view(),
        }
    }

    fn path(&self) -> &Path {
        match self {
            View::Dir(ref dv) => dv.dir_path(),
            View::File(ref fv) => fv.path(),
        }
    }

    fn file_name(&self) -> String {
        match self {
            View::Dir(ref dv) => dv.dir_file_name(),
            View::File(ref fv) => fv.file_name(),
        }
    }

    fn set_geo(&mut self, ngeo: Rect) {
        match self {
            View::Dir(ref mut dv) => dv.geo = ngeo,
            View::File(ref mut fv) => fv.geo = ngeo,
        }
    }
        
    fn as_dir(&self) -> Option<&DirView> {
        match self {
            View::Dir(ref dv) => Some(dv),
            _ => None,
        }
    }

    fn as_file(&self) -> Option<&FileView> {
        match self {
            View::File(ref fv) => Some(fv),
            _ => None,
        }
    }

    fn as_dir_mut(&mut self) -> Option<&mut DirView> {
        match self {
            View::Dir(ref mut dv) => Some(dv),
            _ => None,
        }
    }

    fn as_file_mut(&mut self) -> Option<&mut FileView> {
        match self {
            View::File(ref mut fv) => Some(fv),
            _ => None,
        }
    }
}

impl From<DirView> for View {
    fn from(dv: DirView) -> View {
        View::Dir(dv)
    }
}

impl From<FileView> for View {
    fn from(fv: FileView) -> View {
        View::File(fv)
    }
}

impl<'a> Drawable<&ColorMap<'a>> for View { 
    fn get_geo(&self) -> Rect {
        match self {
            View::Dir(dv) => dv.get_geo(),
            View::File(fv) => fv.get_geo(),
        }
    }

    fn draw(&mut self, d: &mut impl Canvas, c: &ColorMap) {
        match self {
            View::Dir(ref mut dv) => dv.draw(d, c),
            View::File(ref mut fv) => fv.draw(d, c),
        }
    }
}

impl<'a> Nv<'a> {
    fn new( geo: Rect, 
            dir: PathBuf, 
            colors: ColorMap<'a>, 
            binds: KeyBinds, ) -> Self { 

        let dir = dir.canonicalize().unwrap();

        Self {
            root: RootWin::new(geo.clone()),

            views: {
                let mut vm = ViewMap::new();
                vm.insert(
                    dir.clone().into(),
                    DirView::new(
                        Rect::new(0, 0, geo.w, geo.h),
                        dir.clone()
                    ).into()
                );
                vm
            },
            cur_path: dir,

            views_shown: 3,

            colors: colors,
            binds: binds,
        }
    }


    fn start(&mut self) -> Result {
        let mut orig_pos = self.root.abs_cursor_pos();
        let adjusted     = self.root.ensure_geo();

        orig_pos.1 -= adjusted.1;

        {
            let cv = self.get_dir_mut(0).unwrap();

            cv.scan_dir();
            cv.sort(SortOrder::Name);
            cv.select_first();

            self.ensure_populated(1);
            self.ensure_populated(-(self.views_shown as isize-2));
        }

        self.root.cursor().hide().unwrap();

        // initial draw
        self.draw()?;

        // let mut stdin = self.root.input().read_async().bytes();
        loop {
            let c = self.root.input().read_char().unwrap();

            match self.binds[&c] {
                Action::Quit => break,

                Action::MoveDown(..) |
                Action::MoveUp(..) => {
                    let n = match self.binds[&c] {
                        Action::MoveDown(nn) => nn as isize,
                        Action::MoveUp(nn)   => -(nn as isize),
                        _ => unreachable!(),
                    };

                    let cv = self.get_dir_mut(0).unwrap();

                    if cv.inc_sel(n as isize) != 0 {

                        cv.ensure_sel_in_view();
                        self.ensure_populated(1);

                        self.root.clear();
                    }
                },

                Action::MoveLeft(..) |
                Action::MoveRight(..) => {
                    let n = match self.binds[&c] {
                        Action::MoveLeft(nn) => -(nn as isize),
                        Action::MoveRight(nn)  => nn as isize,
                        _ => unreachable!(),
                    };

                    let steps = self.ensure_populated(n);
                    if steps != 0 {
                        self.cur_path = 
                            self.get_view(steps).unwrap().path().to_owned();

                        self.root.clear();
                    }
                },

                _   => (),
            }
            self.draw()?;
        }

        self.root.cursor().show().unwrap();

        self.end(orig_pos)
    }

    fn draw(&mut self) -> Result {
        let shown = self.views_shown;
        let pre = (shown as isize)-2;

        let width = ((self.root.geo.w as f32 -1.0) / shown as f32).floor() as u16;
        let height = self.root.geo.h;
        
        let a_ofs = -pre;
        let b_ofs = 1;

        for ofs in a_ofs..(b_ofs+1) {
            if let Some(path) = self.traverse_dirs(ofs) {
                if let Some(view) = self.views.get_mut(&path) {
                    view.set_geo(Rect {
                        x: (ofs+pre) as u16 * width, 
                        y: 0,
                        w: width,
                        h: height,
                    });
                    self.root.draw(view, &self.colors);
                }
            }
        }

        Ok(())
    }

    fn end(&mut self, pos: (u16, u16)) -> Result {
        self.root.clear();
        self.root.goto_abs(pos.0, pos.1);

        Ok(())
    }

    fn traverse_dirs(&self, lvl_ofs: isize) -> Option<PathBuf> {
        let mut path = self.cur_path.clone();

        if lvl_ofs > 0 {
            for _ in 0..lvl_ofs {
                path.push(self.views.get(&path)?.as_dir()?.sel_file_name());
            }
        }

        if lvl_ofs < 0 {
            for _ in 0..-lvl_ofs {
                if !path.pop() {
                    return None;
                }
            }
        }

        Some(path)
    }

    #[inline]
    fn get_view(&self, lvl_ofs: isize) -> Option<&View> { 
        self.views.get(&self.traverse_dirs(lvl_ofs)?)
    }

    #[inline]
    fn get_view_mut(&mut self, lvl_ofs: isize) -> Option<&mut View> { 
        self.views.get_mut(&self.traverse_dirs(lvl_ofs)?)
    }

    #[inline]
    fn get_dir(&self, lvl_ofs: isize) -> Option<&DirView> {
        self.get_view(lvl_ofs)?.as_dir()
    }

    #[inline]
    fn get_dir_mut(&mut self, lvl_ofs: isize) -> Option<&mut DirView> {
        self.get_view_mut(lvl_ofs)?.as_dir_mut()
    }

    #[inline]
    fn get_file(&self, lvl_ofs: isize) -> Option<&FileView> {
        self.get_view(lvl_ofs)?.as_file()
    }

    #[inline]
    fn get_file_mut(&mut self, lvl_ofs: isize) -> Option<&mut FileView> {
        self.get_view_mut(lvl_ofs)?.as_file_mut()
    }
    
    fn ensure_populated(&mut self, ofs: isize) -> isize {
        // TODO: manually traverse dirs to not retraverse at every iteration
        
        if ofs > 0 {
            for i in 1..(ofs+1) {
                if let Some(child_path) = self.traverse_dirs(i) {

                    if self.views.contains_key(&child_path) {
                        continue;
                    }

                    if let Some(parent_dir) = self.get_dir(i-1) {
                        match parent_dir.make_selected_view() {

                            Some(View::Dir(mut child)) => {
                                child.scan_dir();
                                child.sort(SortOrder::Name);
                                child.select_first();

                                self.views.insert(child_path, child.into()); 
                            }

                            Some(fv@View::File(..)) => {
                                self.views.insert(child_path, fv);
                            },

                            _ => return i,
                        }
                    } else {
                        return i;
                    }
                }
            }

            return ofs;
        }

        if ofs < 0 {
            for i in (ofs..0).rev() {
                let child = self.get_view(i+1).unwrap();

                let mut parent_path = child.path().to_owned();

                if !parent_path.pop() {
                    return i;
                }

                if !self.views.contains_key(&parent_path) { 
                    let mut parent = child.make_parent_dir_view().unwrap();

                    parent.scan_dir();
                    parent.sort(SortOrder::Name);
                    parent.select_by_name(child.file_name());
                    parent.ensure_sel_in_view();

                    self.views.insert(parent_path.clone(), View::Dir(parent));
                }
            }

            return ofs;
        }

        return 0;
    }
}


fn main() {

    let cpos;
    {
        // Cross
        cpos = Crossterm::new().cursor().pos();
    }

    let mut colors = HashMap::new();

    // TODO: Make style parser
    colors.insert("Selected", ObjectStyle {
        fg_color: None,
        bg_color: None,
        attrs: vec![Attribute::Reverse],
    });
    colors.insert("Directory", ObjectStyle {
        fg_color: Some(Color::Blue),
        bg_color: None,
        attrs: vec![Attribute::Bold],
    });
    colors.insert("File", ObjectStyle {
        fg_color: None,
        bg_color: None,
        attrs: vec![],
    });

    let mut binds = HashMap::new();

    // TODO: Make keybinds config parser
    binds.insert('q', Action::Quit);
    binds.insert('j', Action::MoveDown(1));
    binds.insert('k', Action::MoveUp(1));
    binds.insert('h', Action::MoveLeft(1));
    binds.insert('l', Action::MoveRight(1));

    Nv::new(Rect::new(cpos.0, cpos.1, 90, 5), PathBuf::from(r"./"), colors, binds)
        .start()
        .unwrap();
}
