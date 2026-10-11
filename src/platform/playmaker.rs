use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use parking_lot::{Mutex, RwLock};

use crate::hooks::{HookId, HookManager};
use crate::platform::mono::{MonoField, MonoHandler};

#[derive(Clone)]
pub struct FsmVariable {
    pub object: usize,
    pub owner: usize,
    pub vtable: usize,
    pub value: MonoField,
    pub name: String,
    pub fsm: String,
    pub isFloat: bool,
    pub isGlobal: bool,
    pub locked: f64,
}

pub struct PlayMakerHandler {
    mono: MonoHandler,
    intClass: usize,
    floatClass: usize,
    fsmListField: MonoField,
    fsmField: MonoField,
    fsmNameField: MonoField,
    variablesField: MonoField,
    intArrayField: MonoField,
    floatArrayField: MonoField,
    intValueField: MonoField,
    floatValueField: MonoField,
    variableNameField: MonoField,
    itemsField: MonoField,
    sizeField: MonoField,
    globalsInstanceField: MonoField,
    globalsVariablesField: MonoField,
    intVtable: usize,
    floatVtable: usize,
}

impl PlayMakerHandler {
    pub fn create() -> Result<PlayMakerHandler, String> {
        let mono = MonoHandler::create()?;
        mono.attach();
        let image = mono
            .image("PlayMaker.dll")
            .or_else(|| mono.image("PlayMaker"))
            .ok_or_else(|| "PlayMaker image not loaded".to_owned())?;
        let core = mono
            .image("mscorlib.dll")
            .or_else(|| mono.image("mscorlib"))
            .ok_or_else(|| "mscorlib image not loaded".to_owned())?;
        let fsmClass = mono
            .class(image, "", "PlayMakerFSM")
            .ok_or_else(|| "PlayMakerFSM class missing".to_owned())?;
        let fsmType = mono
            .class(image, "HutongGames.PlayMaker", "Fsm")
            .ok_or_else(|| "Fsm class missing".to_owned())?;
        let fs = mono
            .class(image, "HutongGames.PlayMaker", "FsmVariables")
            .ok_or_else(|| "FsmVariables class missing".to_owned())?;
        let intClass = mono
            .class(image, "HutongGames.PlayMaker", "FsmInt")
            .ok_or_else(|| "FsmInt class missing".to_owned())?;
        let floatClass = mono
            .class(image, "HutongGames.PlayMaker", "FsmFloat")
            .ok_or_else(|| "FsmFloat class missing".to_owned())?;
        let namedClass = mono
            .class(image, "HutongGames.PlayMaker", "NamedVariable")
            .ok_or_else(|| "NamedVariable class missing".to_owned())?;
        let listClass = mono
            .class(core, "System.Collections.Generic", "List`1")
            .ok_or_else(|| "List`1 class missing".to_owned())?;
        let globalsClass = mono
            .class(image, "", "PlayMakerGlobals")
            .ok_or_else(|| "PlayMakerGlobals class missing".to_owned())?;
        let fsmListField = mono
            .field(fsmClass, "fsmList")
            .ok_or_else(|| "PlayMakerFSM.fsmList missing".to_owned())?;
        let fsmField = mono
            .field(fsmClass, "fsm")
            .ok_or_else(|| "PlayMakerFSM.fsm missing".to_owned())?;
        let fsmNameField = mono
            .field(fsmType, "name")
            .ok_or_else(|| "Fsm.name missing".to_owned())?;
        let variablesField = mono
            .field(fsmType, "variables")
            .ok_or_else(|| "Fsm.variables missing".to_owned())?;
        let intArrayField = mono
            .field(fs, "intVariables")
            .ok_or_else(|| "FsmVariables.intVariables missing".to_owned())?;
        let floatArrayField = mono
            .field(fs, "floatVariables")
            .ok_or_else(|| "FsmVariables.floatVariables missing".to_owned())?;
        let intValueField = mono
            .field(intClass, "value")
            .ok_or_else(|| "FsmInt.value missing".to_owned())?;
        let floatValueField = mono
            .field(floatClass, "value")
            .ok_or_else(|| "FsmFloat.value missing".to_owned())?;
        let variableNameField = mono
            .field(namedClass, "name")
            .ok_or_else(|| "NamedVariable.name missing".to_owned())?;
        let itemsField = mono
            .field(listClass, "_items")
            .ok_or_else(|| "List._items missing".to_owned())?;
        let sizeField = mono
            .field(listClass, "_size")
            .ok_or_else(|| "List._size missing".to_owned())?;
        let globalsInstanceField = mono
            .field(globalsClass, "instance")
            .ok_or_else(|| "PlayMakerGlobals.instance missing".to_owned())?;
        let globalsVariablesField = mono
            .field(globalsClass, "variables")
            .ok_or_else(|| "PlayMakerGlobals.variables missing".to_owned())?;
        let intVtable = mono
            .vtable(intClass)
            .ok_or_else(|| "FsmInt vtable missing".to_owned())?;
        let floatVtable = mono
            .vtable(floatClass)
            .ok_or_else(|| "FsmFloat vtable missing".to_owned())?;
        mono.vtable(fsmClass);
        Ok(PlayMakerHandler {
            mono,
            intClass,
            floatClass,
            fsmListField,
            fsmField,
            fsmNameField,
            variablesField,
            intArrayField,
            floatArrayField,
            intValueField,
            floatValueField,
            variableNameField,
            itemsField,
            sizeField,
            globalsInstanceField,
            globalsVariablesField,
            intVtable,
            floatVtable,
        })
    }

    pub fn variables(&self) -> Vec<FsmVariable> {
        let mut variables = Vec::new();
        let list = self.mono.staticObject(&self.fsmListField);
        if list == 0 {
            return variables;
        }
        let items = self.mono.fieldObject(list, &self.itemsField);
        if items == 0 {
            return variables;
        }
        let size = self.mono.fieldInt32(list, &self.sizeField).max(0) as usize;
        let length = self.mono.arrayLength(items).min(size);
        for index in 0..length {
            let component = self.mono.arrayElement(items, index);
            if component == 0 {
                continue;
            }
            let fsm = self.mono.fieldObject(component, &self.fsmField);
            if fsm == 0 {
                continue;
            }
            let fsmName = self.mono.text(self.mono.fieldObject(fsm, &self.fsmNameField));
            let owner = self.mono.fieldObject(fsm, &self.variablesField);
            if owner == 0 {
                continue;
            }
            self.collect(
                &fsmName,
                owner,
                &self.intArrayField,
                &self.intValueField,
                self.intVtable,
                false,
                false,
                &mut variables,
            );
            self.collect(
                &fsmName,
                owner,
                &self.floatArrayField,
                &self.floatValueField,
                self.floatVtable,
                true,
                false,
                &mut variables,
            );
        }
        let globals = self.mono.staticObject(&self.globalsInstanceField);
        if globals != 0 {
            let owner = self.mono.fieldObject(globals, &self.globalsVariablesField);
            if owner != 0 {
                self.collect(
                    "Globals",
                    owner,
                    &self.intArrayField,
                    &self.intValueField,
                    self.intVtable,
                    false,
                    true,
                    &mut variables,
                );
                self.collect(
                    "Globals",
                    owner,
                    &self.floatArrayField,
                    &self.floatValueField,
                    self.floatVtable,
                    true,
                    true,
                    &mut variables,
                );
            }
        }
        variables
    }

    pub fn attach(&self) {
        self.mono.attach();
    }

    pub fn getterAddress(&self, isFloat: bool) -> Option<usize> {
        self.mono.compiled(
            if isFloat { self.floatClass } else { self.intClass },
            "get_Value",
            0,
        )
    }

    pub fn setterAddress(&self, isFloat: bool) -> Option<usize> {
        self.mono.compiled(
            if isFloat { self.floatClass } else { self.intClass },
            "set_Value",
            1,
        )
    }

    pub fn read(&self, variable: &FsmVariable) -> Option<f64> {
        if !self.alive(variable) {
            return None;
        }
        Some(if variable.isFloat {
            self.mono.fieldFloat(variable.object, &variable.value) as f64
        } else {
            self.mono.fieldInt32(variable.object, &variable.value) as f64
        })
    }

    pub fn write(&self, variable: &FsmVariable, value: f64) -> bool {
        if !self.alive(variable) {
            return false;
        }
        if variable.isFloat {
            self.mono
                .setFieldFloat(variable.object, &variable.value, value as f32);
        } else {
            self.mono
                .setFieldInt32(variable.object, &variable.value, value.round() as i32);
        }
        true
    }

    pub fn alive(&self, variable: &FsmVariable) -> bool {
        variable.object != 0 && self.mono.vtableOf(variable.object) == variable.vtable
    }

    fn collect(
        &self,
        fsmName: &str,
        owner: usize,
        arrayField: &MonoField,
        valueField: &MonoField,
        vtable: usize,
        isFloat: bool,
        isGlobal: bool,
        out: &mut Vec<FsmVariable>,
    ) {
        let array = self.mono.fieldObject(owner, arrayField);
        let length = self.mono.arrayLength(array);
        for index in 0..length {
            let object = self.mono.arrayElement(array, index);
            if object == 0 {
                continue;
            }
            let name = self
                .mono
                .text(self.mono.fieldObject(object, &self.variableNameField));
            if name.is_empty() {
                continue;
            }
            out.push(FsmVariable {
                object,
                owner,
                vtable,
                value: MonoField::new(valueField.handle, valueField.offset),
                name,
                fsm: fsmName.to_owned(),
                isFloat,
                isGlobal,
                locked: 0.0,
            });
        }
    }
}

static INT_LOCKS: RwLock<Vec<(usize, i32)>> = RwLock::new(Vec::new());
static FLOAT_LOCKS: RwLock<Vec<(usize, f32)>> = RwLock::new(Vec::new());
static INT_BOOSTS: RwLock<Vec<(usize, f64)>> = RwLock::new(Vec::new());
static FLOAT_BOOSTS: RwLock<Vec<(usize, f64)>> = RwLock::new(Vec::new());
static INT_FORCES: RwLock<Vec<(usize, i32)>> = RwLock::new(Vec::new());
static FLOAT_FORCES: RwLock<Vec<(usize, f32)>> = RwLock::new(Vec::new());
static INT_GET_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static FLOAT_GET_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static INT_SET_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static FLOAT_SET_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static INT_HITS: AtomicU64 = AtomicU64::new(0);
static FLOAT_HITS: AtomicU64 = AtomicU64::new(0);
static INT_CALLS: AtomicU64 = AtomicU64::new(0);
static FLOAT_CALLS: AtomicU64 = AtomicU64::new(0);
static INT_BOOST_HITS: AtomicU64 = AtomicU64::new(0);
static FLOAT_BOOST_HITS: AtomicU64 = AtomicU64::new(0);
static INT_FORCE_HITS: AtomicU64 = AtomicU64::new(0);
static FLOAT_FORCE_HITS: AtomicU64 = AtomicU64::new(0);
static NAMED: RwLock<Vec<(usize, String, AtomicU64)>> = RwLock::new(Vec::new());
static TRACKING: AtomicBool = AtomicBool::new(false);
static HOOKS: Mutex<HookState> = Mutex::new(HookState::new());

struct HookIds {
    intGet: Option<HookId>,
    floatGet: Option<HookId>,
    intSet: Option<HookId>,
    floatSet: Option<HookId>,
}

struct HookState {
    users: u64,
    ids: Option<HookIds>,
}

impl HookState {
    const fn new() -> HookState {
        HookState { users: 0, ids: None }
    }
}

pub fn lockValues(variables: &[FsmVariable]) {
    let mut ints = Vec::new();
    let mut floats = Vec::new();
    for variable in variables {
        if variable.isFloat {
            floats.push((variable.object, variable.locked as f32));
        } else {
            ints.push((variable.object, variable.locked.round() as i32));
        }
    }
    *INT_LOCKS.write() = ints;
    *FLOAT_LOCKS.write() = floats;
}

pub fn clearValues() {
    INT_LOCKS.write().clear();
    FLOAT_LOCKS.write().clear();
}

pub fn boostValues(variables: &[FsmVariable], multiplier: f64) {
    let mut ints = Vec::new();
    let mut floats = Vec::new();
    for variable in variables {
        if variable.isFloat {
            floats.push((variable.object, multiplier));
        } else {
            ints.push((variable.object, multiplier));
        }
    }
    *INT_BOOSTS.write() = ints;
    *FLOAT_BOOSTS.write() = floats;
}

pub fn clearBoosts() {
    INT_BOOSTS.write().clear();
    FLOAT_BOOSTS.write().clear();
}

pub fn forceValues(variables: &[FsmVariable], value: f64) {
    let mut ints = Vec::new();
    let mut floats = Vec::new();
    for variable in variables {
        if variable.isFloat {
            floats.push((variable.object, value as f32));
        } else {
            ints.push((variable.object, value.round() as i32));
        }
    }
    *INT_FORCES.write() = ints;
    *FLOAT_FORCES.write() = floats;
}

pub fn clearForces() {
    INT_FORCES.write().clear();
    FLOAT_FORCES.write().clear();
}

pub fn hits() -> (u64, u64) {
    (
        INT_HITS.load(Ordering::Relaxed),
        FLOAT_HITS.load(Ordering::Relaxed),
    )
}

pub fn boostHits() -> (u64, u64) {
    (
        INT_BOOST_HITS.load(Ordering::Relaxed),
        FLOAT_BOOST_HITS.load(Ordering::Relaxed),
    )
}

pub fn forceHits() -> (u64, u64) {
    (
        INT_FORCE_HITS.load(Ordering::Relaxed),
        FLOAT_FORCE_HITS.load(Ordering::Relaxed),
    )
}

pub fn calls() -> (u64, u64) {
    (
        INT_CALLS.load(Ordering::Relaxed),
        FLOAT_CALLS.load(Ordering::Relaxed),
    )
}

pub fn trackNames(variables: &[FsmVariable]) {
    let mut named = NAMED.write();
    named.clear();
    for variable in variables {
        named.push((variable.object, variable.name.clone(), AtomicU64::new(0)));
    }
}

pub fn setTracking(enabled: bool) {
    TRACKING.store(enabled, Ordering::Release);
    if !enabled {
        NAMED.write().clear();
    }
}

pub fn topCounts(limit: usize) -> Vec<(String, u64)> {
    let mut entries: Vec<(String, u64)> = NAMED
        .read()
        .iter()
        .map(|entry| (entry.1.clone(), entry.2.load(Ordering::Relaxed)))
        .filter(|entry| entry.1 > 0)
        .collect();
    entries.sort_by(|left, right| right.1.cmp(&left.1));
    entries.truncate(limit);
    entries
}

fn countCall(object: usize) {
    if !TRACKING.load(Ordering::Relaxed) {
        return;
    }
    if let Some(entry) = NAMED.read().iter().find(|entry| entry.0 == object) {
        entry.2.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn requireHooks(
    handler: &PlayMakerHandler,
    hooks: &mut HookManager,
) -> Result<(), String> {
    let mut state = HOOKS.lock();
    if state.users > 0 {
        state.users += 1;
        return Ok(());
    }
    let mut ids = HookIds {
        intGet: None,
        floatGet: None,
        intSet: None,
        floatSet: None,
    };
    let prepared = (|| -> Result<(), String> {
        if let Some(address) = handler.getterAddress(false) {
            let id = hooks
                .install_raw(address, keepIntValue as *const ())
                .map_err(|error| format!("FsmInt getter hook failed: {error}"))?;
            INT_GET_ORIGINAL.store(hooks.trampoline(id) as usize, Ordering::Release);
            ids.intGet = Some(id);
        }
        if let Some(address) = handler.getterAddress(true) {
            let id = hooks
                .install_raw(address, keepFloatValue as *const ())
                .map_err(|error| format!("FsmFloat getter hook failed: {error}"))?;
            FLOAT_GET_ORIGINAL.store(hooks.trampoline(id) as usize, Ordering::Release);
            ids.floatGet = Some(id);
        }
        if let Some(address) = handler.setterAddress(false) {
            let id = hooks
                .install_raw(address, boostIntValue as *const ())
                .map_err(|error| format!("FsmInt setter hook failed: {error}"))?;
            INT_SET_ORIGINAL.store(hooks.trampoline(id) as usize, Ordering::Release);
            ids.intSet = Some(id);
        }
        if let Some(address) = handler.setterAddress(true) {
            let id = hooks
                .install_raw(address, boostFloatValue as *const ())
                .map_err(|error| format!("FsmFloat setter hook failed: {error}"))?;
            FLOAT_SET_ORIGINAL.store(hooks.trampoline(id) as usize, Ordering::Release);
            ids.floatSet = Some(id);
        }
        Ok(())
    })();
    if let Err(error) = prepared {
        uninstallIds(hooks, &ids);
        resetTrampolines();
        return Err(error);
    }
    INT_HITS.store(0, Ordering::Release);
    FLOAT_HITS.store(0, Ordering::Release);
    INT_CALLS.store(0, Ordering::Release);
    FLOAT_CALLS.store(0, Ordering::Release);
    INT_BOOST_HITS.store(0, Ordering::Release);
    FLOAT_BOOST_HITS.store(0, Ordering::Release);
    INT_FORCE_HITS.store(0, Ordering::Release);
    FLOAT_FORCE_HITS.store(0, Ordering::Release);
    state.ids = Some(ids);
    state.users = 1;
    Ok(())
}

pub fn releaseHooks(hooks: &mut HookManager) {
    let mut state = HOOKS.lock();
    if state.users == 0 {
        return;
    }
    state.users -= 1;
    if state.users > 0 {
        return;
    }
    if let Some(ids) = state.ids.take() {
        uninstallIds(hooks, &ids);
    }
    resetTrampolines();
    clearValues();
    clearBoosts();
    clearForces();
}

fn uninstallIds(hooks: &mut HookManager, ids: &HookIds) {
    if let Some(id) = ids.intGet {
        hooks.uninstall(id);
    }
    if let Some(id) = ids.floatGet {
        hooks.uninstall(id);
    }
    if let Some(id) = ids.intSet {
        hooks.uninstall(id);
    }
    if let Some(id) = ids.floatSet {
        hooks.uninstall(id);
    }
}

fn resetTrampolines() {
    INT_GET_ORIGINAL.store(0, Ordering::Release);
    FLOAT_GET_ORIGINAL.store(0, Ordering::Release);
    INT_SET_ORIGINAL.store(0, Ordering::Release);
    FLOAT_SET_ORIGINAL.store(0, Ordering::Release);
}

unsafe extern "system" fn keepIntValue(object: usize) -> i32 {
    INT_CALLS.fetch_add(1, Ordering::Relaxed);
    countCall(object);
    let forced = INT_FORCES
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1);
    if let Some(value) = forced {
        INT_FORCE_HITS.fetch_add(1, Ordering::Relaxed);
        return value;
    }
    let locked = INT_LOCKS
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1);
    if let Some(value) = locked {
        INT_HITS.fetch_add(1, Ordering::Relaxed);
        return value;
    }
    let original = INT_GET_ORIGINAL.load(Ordering::Acquire);
    if original == 0 {
        return 0;
    }
    let original: unsafe extern "system" fn(usize) -> i32 = unsafe { std::mem::transmute(original) };
    unsafe { original(object) }
}

unsafe extern "system" fn keepFloatValue(object: usize) -> f32 {
    FLOAT_CALLS.fetch_add(1, Ordering::Relaxed);
    countCall(object);
    let forced = FLOAT_FORCES
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1);
    if let Some(value) = forced {
        FLOAT_FORCE_HITS.fetch_add(1, Ordering::Relaxed);
        return value;
    }
    let locked = FLOAT_LOCKS
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1);
    if let Some(value) = locked {
        FLOAT_HITS.fetch_add(1, Ordering::Relaxed);
        return value;
    }
    let original = FLOAT_GET_ORIGINAL.load(Ordering::Acquire);
    if original == 0 {
        return 0.0;
    }
    let original: unsafe extern "system" fn(usize) -> f32 = unsafe { std::mem::transmute(original) };
    unsafe { original(object) }
}

unsafe extern "system" fn boostIntValue(object: usize, value: i32) {
    let mut target = value;
    let forced = INT_FORCES
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1);
    if let Some(value) = forced {
        INT_FORCE_HITS.fetch_add(1, Ordering::Relaxed);
        target = value;
    } else if let Some(factor) = INT_BOOSTS
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1)
    {
        if factor > 1.0 {
            let getter = INT_GET_ORIGINAL.load(Ordering::Acquire);
            if getter != 0 {
                let getValue: unsafe extern "system" fn(usize) -> i32 =
                    unsafe { std::mem::transmute(getter) };
                let current = unsafe { getValue(object) };
                let delta = value.wrapping_sub(current);
                if delta < 0 {
                    let scaled = current as f64 + delta as f64 * factor;
                    target = if value <= 0 {
                        value
                    } else {
                        (scaled.round() as i32).max(0)
                    };
                    INT_BOOST_HITS.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    let setter = INT_SET_ORIGINAL.load(Ordering::Acquire);
    if setter == 0 {
        return;
    }
    let setValue: unsafe extern "system" fn(usize, i32) = unsafe { std::mem::transmute(setter) };
    unsafe { setValue(object, target) };
}

unsafe extern "system" fn boostFloatValue(object: usize, value: f32) {
    let mut target = value;
    let forced = FLOAT_FORCES
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1);
    if let Some(value) = forced {
        FLOAT_FORCE_HITS.fetch_add(1, Ordering::Relaxed);
        target = value;
    } else if let Some(factor) = FLOAT_BOOSTS
        .read()
        .iter()
        .find(|entry| entry.0 == object)
        .map(|entry| entry.1)
    {
        if factor > 1.0 {
            let getter = FLOAT_GET_ORIGINAL.load(Ordering::Acquire);
            if getter != 0 {
                let getValue: unsafe extern "system" fn(usize) -> f32 =
                    unsafe { std::mem::transmute(getter) };
                let current = unsafe { getValue(object) };
                let delta = value - current;
                if delta < 0.0 {
                    let scaled = current as f64 + delta as f64 * factor;
                    target = if value <= 0.0 {
                        value
                    } else {
                        (scaled as f32).max(0.0)
                    };
                    FLOAT_BOOST_HITS.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
    let setter = FLOAT_SET_ORIGINAL.load(Ordering::Acquire);
    if setter == 0 {
        return;
    }
    let setValue: unsafe extern "system" fn(usize, f32) = unsafe { std::mem::transmute(setter) };
    unsafe { setValue(object, target) };
}
