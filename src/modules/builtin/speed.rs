use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use parking_lot::RwLock;

use crate::hooks::HookId;
use crate::modules::bus::EventKind;
use crate::modules::context::ModuleContext;
use crate::modules::harness::Harness;
use crate::modules::info::{ModuleCategory, ModuleInfo};
use crate::modules::runtime::Module;
use crate::modules::schema::ConfigSchema;
use crate::platform::il2cpp::Il2CppHandler;
use crate::utility::math::MathUtility;

const NAME: &str = "Speed";
const FLAGS: u32 = EventKind::Update.flag()
    | EventKind::Open.flag()
    | EventKind::Close.flag()
    | EventKind::Config.flag();
const DEFAULT_MULTIPLIER: f64 = 2.0;
const REPORT_TICKS: u64 = 120;
const IMAGE_NAME: &str = "Assembly-CSharp.dll";
const CLASS_NAMESPACE: &str = "ProjectTarkovH";
const CLASS_NAME: &str = "CharacterController";
const METHOD_NAME: &str = "UpdateVelocity";
const METHOD_PARAMETERS: i32 = 2;
const ROOT_MOTION_FIELD: &str = "_pendingRootMotionLocalDelta";
const ROOT_MOTION_AXES: usize = 3;
const SCHEMA: &[ConfigSchema] = &[ConfigSchema::range(
    "multiplier",
    "速度倍率",
    DEFAULT_MULTIPLIER,
    1.0,
    5.0,
    0.1,
)];

static MULTIPLIER: AtomicU64 = AtomicU64::new(DEFAULT_MULTIPLIER.to_bits());
static ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static ROOT_MOTION_OFFSET: AtomicUsize = AtomicUsize::new(0);
static HOOK_HITS: AtomicU64 = AtomicU64::new(0);
static SPEED_SAMPLE: AtomicU64 = AtomicU64::new(0);

pub fn harness() -> Harness {
    Harness::new(
        ModuleInfo::new(
            NAME,
            "放大角色根位移，提高玩家行走、奔跑与空中移动速度",
            ModuleCategory::Movement,
            0,
            FLAGS,
        ),
        |context| Box::new(SpeedModule::new(context)),
    )
}

pub struct SpeedModule {
    context: RwLock<ModuleContext>,
    hook: RwLock<Option<HookId>>,
    ticks: AtomicU64,
    signature: RwLock<String>,
}

impl SpeedModule {
    fn new(context: &ModuleContext) -> SpeedModule {
        SpeedModule {
            context: RwLock::new(context.clone()),
            hook: RwLock::new(None),
            ticks: AtomicU64::new(0),
            signature: RwLock::new(String::new()),
        }
    }

    fn apply(&self, context: &ModuleContext) {
        let Some(state) = context.state() else {
            return;
        };
        let value: f64 = state.read("multiplier", DEFAULT_MULTIPLIER);
        let clamped = MathUtility::clamp(value as f32, 1.0, 5.0) as f64;
        MULTIPLIER.store(clamped.to_bits(), Ordering::Release);
    }

    fn install(&self, context: &ModuleContext) -> Result<(), String> {
        if self.hook.read().is_some() {
            return Ok(());
        }
        let handler = Il2CppHandler::create()?;
        handler.attach();
        let image = handler
            .image(IMAGE_NAME)
            .ok_or_else(|| format!("{IMAGE_NAME} image missing"))?;
        let class = handler
            .class(image, CLASS_NAMESPACE, CLASS_NAME)
            .ok_or_else(|| format!("{CLASS_NAME} class missing"))?;
        let field = handler
            .field(class, ROOT_MOTION_FIELD)
            .ok_or_else(|| format!("{ROOT_MOTION_FIELD} field missing"))?;
        let method = handler
            .method(class, METHOD_NAME, METHOD_PARAMETERS)
            .ok_or_else(|| format!("{METHOD_NAME} method missing"))?;
        let host = crate::bootstrap::host().ok_or_else(|| "host unavailable".to_owned())?;
        let hooks = host.hooks();
        let id = hooks
            .write()
            .install_raw(method.address, speedHook as *const ())
            .map_err(|error| format!("speed hook failed: {error}"))?;
        let trampoline = hooks.read().trampoline(id) as usize;
        HOOK_HITS.store(0, Ordering::Release);
        SPEED_SAMPLE.store(0, Ordering::Release);
        ROOT_MOTION_OFFSET.store(field.offset, Ordering::Release);
        ORIGINAL.store(trampoline, Ordering::Release);
        *self.hook.write() = Some(id);
        if let Some(logger) = context.logger() {
            logger.info(&format!(
                "speed hook at {:#x}, multiplier {:.2}, root motion offset {:#x}",
                method.address,
                multiplier(),
                field.offset
            ));
        }
        Ok(())
    }

    fn removeHooks(&self) {
        ORIGINAL.store(0, Ordering::Release);
        ROOT_MOTION_OFFSET.store(0, Ordering::Release);
        let Some(id) = self.hook.write().take() else {
            return;
        };
        if let Some(host) = crate::bootstrap::host() {
            host.hooks().write().uninstall(id);
        }
    }

    fn refresh(&self, context: &ModuleContext) {
        if self.hook.read().is_some() {
            return;
        }
        match self.install(context) {
            Ok(()) => {}
            Err(error) => {
                if *self.signature.read() == error {
                    return;
                }
                *self.signature.write() = error.clone();
                if let Some(logger) = context.logger() {
                    logger.warn(&format!("speed unavailable: {error}"));
                }
            }
        }
    }
}

impl Module for SpeedModule {
    fn names(&self) -> &'static str {
        NAME
    }

    fn context(&self) -> &RwLock<ModuleContext> {
        &self.context
    }

    fn schema(&self) -> &'static [ConfigSchema] {
        SCHEMA
    }

    fn on_enable(&self, context: &ModuleContext) {
        self.apply(context);
        self.ticks.store(0, Ordering::Release);
        self.signature.write().clear();
        self.refresh(context);
    }

    fn on_disable(&self, context: &ModuleContext) {
        self.removeHooks();
        if let Some(logger) = context.logger() {
            logger.info("speed disabled");
        }
    }

    fn on_config(&self, context: &ModuleContext, key: &str) {
        if key == "multiplier" {
            self.apply(context);
            if let Some(logger) = context.logger() {
                logger.info(&format!("speed multiplier {:.2}", multiplier()));
            }
        }
    }

    fn on_event(&self, context: &mut ModuleContext, kind: EventKind) {
        if kind != EventKind::Update {
            return;
        }
        let ticks = self.ticks.fetch_add(1, Ordering::AcqRel) + 1;
        if ticks % REPORT_TICKS != 0 {
            return;
        }
        self.refresh(context);
        if self.hook.read().is_none() {
            return;
        }
        if let Some(logger) = context.logger() {
            logger.debug(&format!(
                "speed hook hits={} speed={:.2}",
                HOOK_HITS.load(Ordering::Relaxed),
                f64::from_bits(SPEED_SAMPLE.load(Ordering::Relaxed))
            ));
        }
    }
}

fn multiplier() -> f64 {
    f64::from_bits(MULTIPLIER.load(Ordering::Acquire))
}

unsafe extern "system" fn speedHook(instance: usize, currentVelocity: *mut f32, deltaTime: f32) {
    let original = ORIGINAL.load(Ordering::Acquire);
    if original == 0 {
        return;
    }
    let original: unsafe extern "system" fn(usize, *mut f32, f32) =
        unsafe { std::mem::transmute(original) };
    HOOK_HITS.fetch_add(1, Ordering::Relaxed);
    if !currentVelocity.is_null() {
        let horizontal = unsafe {
            let x = std::ptr::read(currentVelocity);
            let z = std::ptr::read(currentVelocity.add(2));
            (x * x + z * z).sqrt()
        };
        SPEED_SAMPLE.store((horizontal as f64).to_bits(), Ordering::Relaxed);
    }
    let rootMotion = ROOT_MOTION_OFFSET.load(Ordering::Acquire);
    let factor = multiplier() as f32;
    if instance == 0 || rootMotion == 0 || factor == 1.0 {
        unsafe {
            original(instance, currentVelocity, deltaTime);
        }
        return;
    }
    for axis in 0..ROOT_MOTION_AXES {
        let address = (instance + rootMotion + axis * 4) as *mut f32;
        let value = unsafe { std::ptr::read(address) };
        unsafe {
            std::ptr::write(address, value * factor);
        }
    }
    unsafe {
        original(instance, currentVelocity, deltaTime);
    }
}
