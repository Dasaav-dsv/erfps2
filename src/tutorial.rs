use eldenring::{
    cs::{CSMenuManImp, CSPopupMenu, MsgRepository, TpfRepository},
    fd4::FD4ParamRepository,
    param::TUTORIAL_PARAM_ST,
};
use fromsoftware_shared::FromStatic;

use crate::{program::Program, rva::SHOW_TUTORIAL_POPUP, tutorial::tpf::load_tutorial_tpf};

mod tpf;

pub const TUTORIAL_EVENT_FLAG_ID: u32 = 69009;

const TUTORIAL_ROW_ID: u32 = 1000;
const TUTORIAL_MSG_ID: u32 = 302420;
const TUTORIAL_IMG_ID: u16 = 18949;

const TUTORIAL_TITLE: &str = "ERFPS (Ver.2)";
const TUTORIAL_TEXT: &str = r#"Thank you for installing ERFPS!
(This is a free and open source mod.)

To switch between 1st and 3rd person hold Interact and press Lock on.

<?keyicon@27?>+<?keyicon@15?>: Switch perspectives"#;

pub fn show_tutorial() {
    std::thread::spawn(show_tutorial_blocking);
}

struct OriginalContents<'r, 't> {
    row: &'r mut TUTORIAL_PARAM_ST,
    original_row: TUTORIAL_PARAM_ST,
    title: &'t mut [u16],
    original_title: Box<[u16]>,
    text: &'t mut [u16],
    original_text: Box<[u16]>,
}

pub fn show_tutorial_blocking() -> Option<bool> {
    let param_repository = unsafe { FD4ParamRepository::instance_mut().ok()? };
    let tutorial_param_row =
        unsafe { param_repository.get_mut::<TUTORIAL_PARAM_ST>(TUTORIAL_ROW_ID)? };

    let msg_repository = unsafe { MsgRepository::instance_mut().ok()? };
    let [tutorial_title, tutorial_text] =
        msg_repository.get_msg_disjoint_mut([(207, TUTORIAL_MSG_ID), (208, TUTORIAL_MSG_ID)])?;

    let menu_man = unsafe { CSMenuManImp::instance().ok()? };
    let popup_menu = unsafe { menu_man.popup_menu?.as_mut() };

    let tpf_repo = unsafe { TpfRepository::instance_mut().ok()? };
    load_tutorial_tpf(tpf_repo);

    std::thread::sleep(std::time::Duration::from_millis(800));

    let _contents = modify_tutorial_contents(tutorial_param_row, tutorial_title, tutorial_text);

    let result = unsafe {
        let show_tutorial =
            Program::current()
                .derva_ptr::<unsafe extern "C" fn(*mut CSPopupMenu, u32, bool, bool) -> bool>(
                    SHOW_TUTORIAL_POPUP,
                );

        show_tutorial(popup_menu, TUTORIAL_ROW_ID, true, true)
    };

    std::thread::sleep(std::time::Duration::from_millis(200));

    Some(result)
}

fn modify_tutorial_contents<'r, 't>(
    row: &'r mut TUTORIAL_PARAM_ST,
    title: &'t mut [u16],
    text: &'t mut [u16],
) -> OriginalContents<'r, 't> {
    let original_row = row.clone();
    let original_title = Box::from(&mut *title);
    let original_text = Box::from(&mut *text);

    row.set_menu_type(100);
    row.set_trigger_type(0);
    row.set_repeat_type(1);
    row.set_image_id(TUTORIAL_IMG_ID);
    row.set_unlock_event_flag_id(TUTORIAL_EVENT_FLAG_ID);
    row.set_text_id(TUTORIAL_MSG_ID as i32);
    row.set_display_min_time(1.0);
    row.set_display_time(3.0);

    let title_len = (TUTORIAL_TITLE.len() + 1).min(title.len());
    title[..title_len].copy_from_slice(
        &TUTORIAL_TITLE.encode_utf16().chain([0]).collect::<Vec<_>>()[..title_len],
    );

    let text_len = (TUTORIAL_TEXT.len() + 1).min(text.len());
    text[..text_len]
        .copy_from_slice(&TUTORIAL_TEXT.encode_utf16().chain([0]).collect::<Vec<_>>()[..text_len]);

    OriginalContents {
        row,
        original_row,
        title,
        original_title,
        text,
        original_text,
    }
}

impl Drop for OriginalContents<'_, '_> {
    fn drop(&mut self) {
        *self.row = self.original_row.clone();
        self.title.copy_from_slice(&self.original_title);
        self.text.copy_from_slice(&self.original_text);
    }
}
