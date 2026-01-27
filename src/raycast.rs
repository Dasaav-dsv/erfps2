use std::{
    array,
    ffi::c_void,
    fmt,
    mem::MaybeUninit,
    ptr::{self, NonNull},
};

use eldenring::cs::{CSHavokMan, CSPhysIns, FieldInsBase, FieldInsHandle, hknpWorld};
use fromsoftware_shared::FromStatic;
use glam::{Mat4, Vec3, Vec3A, Vec4};

use crate::{
    program::Program,
    rva::{CAM_HIT_COLLECTOR_RVA, CAST_SHAPE_RVA, HKNP_SPHERE_SHAPE_RVA},
};

pub fn cast_sphere<F>(
    origin: Vec3,
    direction: Vec3,
    radius: f32,
    filter: u32,
    f: F,
) -> Option<hknpHit>
where
    F: Fn(&hknpHit) -> bool,
{
    cast_shape(origin, direction, filter, hknpSphereShape::new(radius), f)
}

fn cast_shape<T, F>(
    origin: Vec3,
    direction: Vec3,
    filter: u32,
    mut shape: T,
    f: F,
) -> Option<hknpHit>
where
    T: HavokShape,
    F: Fn(&hknpHit) -> bool,
{
    let havok_man = unsafe { CSHavokMan::instance().ok()? };

    let mut params = CastParams::cast_shape(havok_man, origin, direction, filter, &mut shape);

    let mut custom_collector = CustomHitCollector::new(&f);
    let mut collector = ClosestHitCollector::default();

    unsafe {
        let cast_shape = Program::current().derva_ptr::<unsafe extern "C" fn(
            *mut hknpWorld,
            *mut CastParams<'_, T>,
            *const Mat4,
            *mut CustomHitCollector,
            *mut ClosestHitCollector,
        )>(CAST_SHAPE_RVA);

        cast_shape(
            havok_man.phys_world.hknp_world.as_ptr(),
            &mut params,
            &Mat4::IDENTITY,
            &mut custom_collector,
            &mut collector,
        );
    }

    (custom_collector.is_hit != 0).then_some(custom_collector.hit)
}

trait HavokShape {}

#[repr(C)]
struct CastParams<'a, T: HavokShape> {
    shape_tag_filter: *mut c_void,
    collision_filter: *mut c_void,
    unk10: u16,
    filter: u32,
    user_data: *mut c_void,
    unk20: u32,
    unk24: u8,
    shape: Option<&'a mut T>,
    origin: Vec4,
    direction: Vec4,
    direction_rcp: Vec4,
    unk60: f32,
    unk64: u32,
    unk68: u32,
    unk6c: u32,
    unk70: *mut c_void,
    unk78: f32,
    unk80: Vec4,
    unk90: Vec4,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct hknpHit {
    pub pos: Vec3A,
    pub normal: Vec3A,
    pub segment: f32,
    unk24: [u8; 4],
    unk28: u32,
    unk2c: u16,
    unk30: u32,
    pub filter: u32,
    unk38: u32,
    unk3c: [u8; 4],
    unk40: [u8; 0x8],
    pub body_id: hknpBodyId,
    unk4c: u32,
    unk50: u32,
    unk54: u32,
    unk58: u32,
    body: *mut *mut CSPhysIns,
    unk68: u32,
    unk6c: u32,
}

#[repr(C)]
struct ClosestHitCollector {
    vtable: &'static ClosestHitCollectorVtable,
    unk08: u8,
    is_hit: u32,
    segment: Vec4,
    unk20: u8,
    hit: hknpHit,
}

#[repr(C)]
struct CustomHitCollector<'a> {
    vtable: Box<CustomHitCollectorVtable<'a>>,
    unk08: u8,
    is_hit: u32,
    segment: Vec4,
    unk20: u8,
    hit: hknpHit,
}

#[repr(C)]
#[derive(Clone)]
struct ClosestHitCollectorVtable {
    hk_reflection: unsafe extern "C" fn(*const ClosestHitCollector, *mut c_void) -> *mut c_void,
    drop: unsafe extern "C" fn(*mut ClosestHitCollector, u8),
    drop_in_place: unsafe extern "C" fn(*mut ClosestHitCollector),
    collision: unsafe extern "C" fn(*mut ClosestHitCollector) -> *mut Vec3A,
    filter: unsafe extern "C" fn(*mut ClosestHitCollector, *const hknpHit),
    unk28: unsafe extern "C" fn(*mut ClosestHitCollector),
}

#[repr(C)]
struct CustomHitCollectorVtable<'a> {
    base: ClosestHitCollectorVtable,
    original_filter: unsafe extern "C" fn(*mut ClosestHitCollector, *const hknpHit),
    custom_filter: &'a dyn Fn(&hknpHit) -> bool,
}

#[repr(C)]
struct hknpSphereShape {
    vtable: usize,
    unk08: [u8; 0x8],
    size_bytes: u16,
    unk0e: [u8; 0xe],
    radius: f32,
    unk24: [u8; 0x16],
    vertex_count: u16,
    verices: [Vec4; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct hknpBodyId(u32);

impl<'a, T: HavokShape> CastParams<'a, T> {
    fn cast_ray(havok_man: &mut CSHavokMan, origin: Vec3, direction: Vec3, filter: u32) -> Self {
        let direction_rcp = Vec3::from_array(array::from_fn(|i| match direction[i] {
            0.0 => f32::MAX,
            e => e.recip(),
        }));

        Self {
            shape_tag_filter: havok_man.phys_world.hknp_world.shape_tag_filter,
            collision_filter: havok_man.collision_filter,
            unk10: u16::MAX,
            filter,
            user_data: ptr::null_mut(),
            unk20: 2,
            unk24: 0xfb,
            shape: None,
            origin: origin.extend(1.0),
            direction: direction.extend(1.0),
            direction_rcp: direction_rcp.extend(1.0),
            unk60: 0.001,
            unk64: 0,
            unk68: 0,
            unk6c: 0x100,
            unk70: ptr::null_mut(),
            unk78: f32::MAX,
            unk80: Vec4::ZERO,
            unk90: Vec4::ZERO,
        }
    }

    fn cast_shape(
        havok_man: &mut CSHavokMan,
        origin: Vec3,
        direction: Vec3,
        filter: u32,
        shape: &'a mut T,
    ) -> Self {
        Self {
            shape: Some(shape),
            ..Self::cast_ray(havok_man, origin, direction, filter)
        }
    }
}

impl hknpHit {
    pub fn field_ins(&self) -> Option<NonNull<FieldInsBase>> {
        unsafe { Some(self.body.as_ref()?.as_ref()?.owner) }
    }

    pub fn field_ins_handle(&self) -> Option<FieldInsHandle> {
        unsafe { Some(self.field_ins()?.as_ref().handle) }
    }
}

impl<'a> CustomHitCollector<'a> {
    fn new(custom_filter: &'a dyn Fn(&hknpHit) -> bool) -> Self {
        let base = ClosestHitCollector::default();

        let vtable = CustomHitCollectorVtable {
            base: ClosestHitCollectorVtable {
                filter: Self::filter,
                ..*base.vtable
            },
            custom_filter,
            original_filter: base.vtable.filter,
        };

        Self {
            vtable: Box::new(vtable),
            unk08: base.unk08,
            is_hit: base.is_hit,
            segment: base.segment,
            unk20: base.unk20,
            hit: base.hit,
        }
    }

    unsafe extern "C" fn filter(collector: *mut ClosestHitCollector, data: *const hknpHit) {
        let original_filter = unsafe {
            let custom_collector = &mut *collector.cast::<Self>();
            if !(custom_collector.vtable.custom_filter)(&*data) {
                return;
            }
            custom_collector.vtable.original_filter
        };

        unsafe {
            original_filter(collector, data);
        }
    }
}

impl hknpSphereShape {
    fn new(radius: f32) -> Self {
        unsafe {
            let mut uninit = MaybeUninit::<Self>::uninit();
            let uninit_ptr = uninit.as_mut_ptr();

            let ctor = Program::current()
                .derva_ptr::<unsafe extern "C" fn(*mut Self, *const Vec4, f32)>(
                    HKNP_SPHERE_SHAPE_RVA,
                );

            (*uninit_ptr).vertex_count = 1;
            ctor(uninit_ptr, &Vec4::ZERO, radius);
            (*uninit_ptr).size_bytes = 0x50;

            uninit.assume_init()
        }
    }
}

impl hknpBodyId {
    fn index(self) -> Option<usize> {
        let id = self.0 & 0xffffff;
        (id != 0xffffff).then_some(id as usize)
    }
}

impl Default for ClosestHitCollector {
    fn default() -> Self {
        unsafe {
            let mut uninit = MaybeUninit::uninit();

            let ctor = Program::current()
                .derva_ptr::<unsafe extern "C" fn(*mut ClosestHitCollector)>(CAM_HIT_COLLECTOR_RVA);
            ctor(uninit.as_mut_ptr());

            uninit.assume_init()
        }
    }
}

impl fmt::Debug for hknpBodyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("hknpBodyId").field(&self.index()).finish()
    }
}

impl HavokShape for hknpSphereShape {}
