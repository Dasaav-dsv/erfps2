use eldenring::cs::{ChrAsmArmStyle, ChrIns, PlayerIns, ThrowNodeState, WorldChrMan};
use fromsoftware_shared::{F32ModelMatrix, F32Vector4, F32ViewMatrix, FromStatic};
use glam::Vec4;

use crate::{
    program::Program,
    rva::{GET_DMY_POS_RVA, IS_CHR_RIDING_RVA},
};

pub trait PlayerExt {
    fn main_player<'a>() -> Option<&'a mut Self>;

    fn model_mtx(&self) -> F32ModelMatrix;

    fn head_position(&self) -> F32ModelMatrix;

    fn aim_mtx_mut(&mut self) -> &mut F32ViewMatrix;

    fn set_lock_on(&mut self, state: bool);

    fn set_lock_on_target_position(&mut self, pos: Vec4);

    fn enable_face_model(&mut self, state: bool);

    fn is_sprinting(&self) -> bool;

    fn is_riding(&self) -> bool;

    fn is_on_ladder(&self) -> bool;

    fn is_in_throw(&self) -> bool;

    fn is_2h(&self) -> bool;
}

impl PlayerExt for PlayerIns {
    fn main_player<'a>() -> Option<&'a mut Self> {
        let world_chr_man = unsafe { WorldChrMan::instance().ok()? };
        world_chr_man.main_player.as_deref_mut()
    }

    fn model_mtx(&self) -> F32ModelMatrix {
        let mut m = self.chr_ctrl.model_matrix;
        m.0 = F32Vector4(0.0, 0.0, 0.0, 0.0) - m.0;
        m.2 = F32Vector4(0.0, 0.0, 0.0, 0.0) - m.2;
        m
    }

    fn head_position(&self) -> F32ModelMatrix {
        type GetDmyPos = unsafe extern "C" fn(
            *const ChrIns,
            *mut F32ModelMatrix,
            *const u32,
            i32,
        ) -> *mut F32Vector4;

        // Fetch a model matrix for the head dummy poly in world space.
        const HEAD_DMY_ID: u32 = 907;
        unsafe {
            let get_dmy_pos = Program::current().derva_ptr::<GetDmyPos>(GET_DMY_POS_RVA);

            let mut dmy_pos = F32ModelMatrix::IDENTITY;
            get_dmy_pos(&**self, &mut dmy_pos, &HEAD_DMY_ID, 1);

            dmy_pos
        }
    }

    fn aim_mtx_mut(&mut self) -> &mut F32ViewMatrix {
        &mut self.aim_view_mtx
    }

    fn set_lock_on(&mut self, state: bool) {
        self.is_locked_on = state;
        self.chr_ctrl.is_unlocked = !state;
    }

    fn set_lock_on_target_position(&mut self, pos: Vec4) {
        self.lock_on_target_position = pos.with_w(1.0).into();
    }

    fn enable_face_model(&mut self, state: bool) {
        // Toggle face, helmet, hair, eyes, etc. visibility but still cast a shadow.
        for parts_model_ins in self
            .chr_asm_model_ins
            .iter_mut()
            .flat_map(|ptr| unsafe {
                ptr.parts_model_ins
                    .get_disjoint_unchecked_mut([0, 2, 6, 21, 22, 23, 24, 25])
            })
            .flatten()
        {
            // e.g. 0x100000A1 - visible, casts a shadow
            //      0x100000A0 - invisible, casts a shadow.
            parts_model_ins.model_disp_entity.disp_flags1 =
                parts_model_ins.model_disp_entity.disp_flags1 & !1 | state as u32;
        }

        // Toggle back of head and ears visibility (which otherwise may clip into view).
        if let Some(chr_asm_model_res) = self.chr_asm_model_res {
            // Model mask 3 (bit 3 in the u64 at +0x58).
            let ears_model_mask = unsafe { chr_asm_model_res.add(0x58).as_mut() };
            *ears_model_mask = *ears_model_mask & !8 | (!state as u8) << 3;
            // Model mask 36 (bit 11 in the u64 at +0x48).
            let hood_model_mask = unsafe { chr_asm_model_res.add(0x49).as_mut() };
            *hood_model_mask = *hood_model_mask & !8 | (!state as u8) << 3;
            // Model mask ??
            let hood_model_mask2 = unsafe { chr_asm_model_res.add(0x50).as_mut() };
            *hood_model_mask2 = *hood_model_mask2 & !32 | (!state as u8) << 5;
        }
    }

    fn is_sprinting(&self) -> bool {
        // FIXME: not sure this is reliable?
        self.module_container.action_request.action_timers.roll > 0.3
    }

    fn is_riding(&self) -> bool {
        type IsChrRiding = unsafe extern "C" fn(*const ()) -> bool;
        unsafe {
            let is_chr_riding = Program::current().derva_ptr::<IsChrRiding>(IS_CHR_RIDING_RVA);
            is_chr_riding(self.module_container.ride)
        }
    }

    fn is_on_ladder(&self) -> bool {
        self.module_container.event.ladder_state >= 0
    }

    fn is_in_throw(&self) -> bool {
        matches!(
            self.module_container.throw.throw_node.throw_state,
            ThrowNodeState::InThrowTarget | ThrowNodeState::InThrowAttacker
        )
    }

    fn is_2h(&self) -> bool {
        matches!(
            self.chr_asm.equipment.arm_style,
            ChrAsmArmStyle::LeftBothHands | ChrAsmArmStyle::RightBothHands
        )
    }
}
