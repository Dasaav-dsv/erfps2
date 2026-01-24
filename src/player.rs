use eldenring::{
    cs::{
        CSModelIns, ChrAsmArmStyle, ChrIns, ChrMovementLimit, PlayerIns, ThrowNodeState,
        WorldChrMan,
    },
    fd4::FD4ParamRepository,
    param::EQUIP_PARAM_WEAPON_ST,
};
use fromsoftware_shared::{F32ModelMatrix, F32Vector4, F32ViewMatrix, FromStatic, OwnedPtr};
use glam::{Vec3, Vec4, Vec4Swizzles};

use crate::{program::Program, rva::GET_DMY_POS_RVA};

pub trait PlayerExt {
    fn main_player<'a>() -> Option<&'a mut Self>;

    fn model_mtx(&self) -> F32ModelMatrix;

    fn head_position(&self) -> F32ModelMatrix;

    fn input_move_dir(&self) -> Vec3;

    fn aim_mtx_mut(&mut self) -> &mut F32ViewMatrix;

    fn set_lock_on(&mut self, state: bool);

    fn set_lock_on_target_position(&mut self, pos: Vec4);

    fn make_transparent(&mut self, state: bool);

    fn enable_face_model(&mut self, state: bool);

    fn enable_sheathed_weapons(&mut self, state: bool);

    fn cancel_sprint(&mut self);

    fn has_action_request(&self) -> bool;

    fn is_sprinting(&self) -> bool;

    fn is_sprint_requested(&self) -> bool;

    fn is_riding(&self) -> bool;

    fn is_approaching_ladder(&self) -> bool;

    fn is_in_throw(&self) -> bool;

    fn is_2h(&self) -> bool;

    fn lh_weapon_param(&self) -> Option<&'static EQUIP_PARAM_WEAPON_ST>;

    fn rh_weapon_param(&self) -> Option<&'static EQUIP_PARAM_WEAPON_ST>;
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

    fn input_move_dir(&self) -> Vec3 {
        -Vec4::from(self.chr_ctrl.input_move_dir).zyx()
    }

    fn set_lock_on(&mut self, state: bool) {
        self.is_locked_on = state;
        self.chr_ctrl.is_unlocked = !state;
    }

    fn set_lock_on_target_position(&mut self, pos: Vec4) {
        self.lock_on_target_position = pos.with_w(1.0).into();
    }

    fn make_transparent(&mut self, state: bool) {
        // Magic constant (about 0.4683).
        // Prevents overwriting values other than our bit pattern.
        const MAGIC: f32 = -f32::from_bits(0xbeefbeef);

        if state && self.base_transparency == 1.0 {
            self.base_transparency = MAGIC;
        }

        if !state && self.base_transparency == MAGIC {
            self.base_transparency = 1.0;
        }
    }

    fn enable_face_model(&mut self, state: bool) {
        // Toggle face, helmet, hair, eyes, etc. visibility but still cast a shadow.
        for parts in self.chr_asm_model_ins.iter_mut().flat_map(|ptr| unsafe {
            ptr.parts_model_ins
                .get_disjoint_unchecked_mut([0, 2, 6, 21, 22, 23, 24, 25])
        }) {
            enable_parts_visibilty(parts, state);
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

    fn enable_sheathed_weapons(&mut self, state: bool) {
        let is_riding = self.is_riding();

        let lh_weapon_param = self.lh_weapon_param();
        let rh_weapon_param = self.rh_weapon_param();

        let Some(chr_asm_model_ins) = self.chr_asm_model_ins.as_mut() else {
            return;
        };

        let parts = unsafe {
            chr_asm_model_ins
                .parts_model_ins
                .get_disjoint_unchecked_mut([7, 8, 11, 12])
        };

        if state {
            for part in parts {
                enable_parts_visibilty(part, true);
            }

            return;
        }

        let [lh_weapon, lh_sheath, rh_weapon, rh_sheath] = parts;

        let (lh_weapon_visibility, rh_weapon_visibility) = match self.chr_asm.equipment.arm_style {
            ChrAsmArmStyle::OneHanded if is_riding => (false, true),
            ChrAsmArmStyle::LeftBothHands => (true, false),
            ChrAsmArmStyle::RightBothHands => (false, true),
            _ => (true, true),
        };

        let lh_sheath_visibility = lh_weapon_param.is_some_and(|row| row.is_dual_blade() != 0);
        let rh_sheath_visibility = rh_weapon_param.is_some_and(|row| row.is_dual_blade() != 0);

        enable_parts_visibilty(lh_weapon, lh_weapon_visibility);
        enable_parts_visibilty(rh_weapon, rh_weapon_visibility);
        enable_parts_visibilty(lh_sheath, lh_sheath_visibility);
        enable_parts_visibilty(rh_sheath, rh_sheath_visibility);
    }

    fn cancel_sprint(&mut self) {
        if self.chr_ctrl.modifier.data.movement_limit == ChrMovementLimit::NoLimit {
            self.chr_ctrl.modifier.data.movement_limit = ChrMovementLimit::LimitToDash;
        }
    }

    fn has_action_request(&self) -> bool {
        self.module_container
            .event
            .ez_state_requests_state
            .iter()
            .any(|i| *i >= 0)
    }

    fn is_sprinting(&self) -> bool {
        self.special_effect
            .entries()
            .any(|sp_effect| sp_effect.param_id == 100002)
    }

    fn is_sprint_requested(&self) -> bool {
        self.module_container.action_request.action_timers.roll > 0.3
    }

    fn is_riding(&self) -> bool {
        self.module_container.ride.is_mounted
    }

    fn is_approaching_ladder(&self) -> bool {
        self.module_container.ladder.ladder_state == 0
            || self.module_container.ladder.ladder_state == 1
    }

    fn is_in_throw(&self) -> bool {
        matches!(
            self.module_container.throw.throw_node.throw_state,
            ThrowNodeState::InThrowTarget | ThrowNodeState::InThrowAttacker
        )
    }

    fn is_2h(&self) -> bool {
        self.chr_asm.equipment.arm_style == ChrAsmArmStyle::RightBothHands
            && self
                .rh_weapon_param()
                .is_none_or(|row| row.is_dual_blade() == 0)
            || self.chr_asm.equipment.arm_style == ChrAsmArmStyle::LeftBothHands
                && self
                    .lh_weapon_param()
                    .is_none_or(|row| row.is_dual_blade() == 0)
    }

    fn lh_weapon_param(&self) -> Option<&'static EQUIP_PARAM_WEAPON_ST> {
        let chr_asm = self.chr_asm.as_ref();

        let lh_slot = chr_asm.equipment.selected_slots.left_weapon_slot * 2;
        let lh_weapon_param_id = chr_asm.equipment_param_ids[lh_slot as usize];

        unsafe {
            FD4ParamRepository::instance()
                .ok()?
                .get(lh_weapon_param_id as u32)
        }
    }

    fn rh_weapon_param(&self) -> Option<&'static EQUIP_PARAM_WEAPON_ST> {
        let chr_asm = self.chr_asm.as_ref();

        let rh_slot = chr_asm.equipment.selected_slots.right_weapon_slot * 2 + 1;
        let rh_weapon_param_id = chr_asm.equipment_param_ids[rh_slot as usize];

        unsafe {
            FD4ParamRepository::instance()
                .ok()?
                .get(rh_weapon_param_id as u32)
        }
    }
}

fn enable_parts_visibilty(parts: &mut Option<OwnedPtr<CSModelIns>>, state: bool) {
    if let Some(parts) = parts {
        // e.g. 0x100000A1 - visible, casts a shadow
        //      0x100000A0 - invisible, casts a shadow.
        parts.model_disp_entity.disp_flags1 =
            parts.model_disp_entity.disp_flags1 & !1 | state as u32;
    }
}
