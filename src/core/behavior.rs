use bitvec::BitArr;
use strum::EnumCount;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumCount)]
pub enum BehaviorState {
    Attack,
    Evasion,
    Gesture,
}

#[derive(Clone, Copy, Default)]
pub struct BehaviorStateSet {
    bits: BitArr!(for BehaviorState::COUNT, in u8),
}

#[derive(Default)]
pub struct BehaviorStates {
    sets: [BehaviorStateSet; 2],
}

impl BehaviorStateSet {
    pub fn set_state(&mut self, state: BehaviorState) {
        self.bits.set(state as usize, true);
    }
}

impl BehaviorStates {
    pub fn has_state(&self, state: BehaviorState) -> bool {
        (self.sets[0].bits | self.sets[1].bits)[state as usize]
    }

    pub fn push_state_set(&mut self, set: BehaviorStateSet) {
        self.sets[1] = self.sets[0];
        self.sets[0] = set;
    }
}

impl BehaviorState {
    pub fn try_from_state_name(name: &str) -> Option<Self> {
        match name {
            "Attack_SM" => Some(Self::Attack),
            "Evasion_SM" | "Stealth_Rolling_CMSG" => Some(Self::Evasion),
            "Gesture_SM" => Some(Self::Gesture),
            _ => None,
        }
    }
}

impl TryFrom<&str> for BehaviorState {
    type Error = ();

    fn try_from(name: &'_ str) -> Result<Self, Self::Error> {
        Self::try_from_state_name(name).ok_or(())
    }
}
