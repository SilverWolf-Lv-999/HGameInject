use std::ffi::{CStr, CString, c_char, c_void};

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

use crate::platform::memory::MemoryUtility;
use crate::platform::runtime::{MONO_MODULES, module_handle};

type MonoGetRootDomain = unsafe extern "system" fn() -> usize;
type MonoThreadAttach = unsafe extern "system" fn(usize) -> usize;
type MonoImageLoaded = unsafe extern "system" fn(*const c_char) -> usize;
type MonoClassFromName = unsafe extern "system" fn(usize, *const c_char, *const c_char) -> usize;
type MonoClassGetFieldFromName = unsafe extern "system" fn(usize, *const c_char) -> usize;
type MonoClassGetMethodFromName = unsafe extern "system" fn(usize, *const c_char, i32) -> usize;
type MonoCompileMethod = unsafe extern "system" fn(usize) -> usize;
type MonoFieldGetOffset = unsafe extern "system" fn(usize) -> u32;
type MonoFieldGetValue = unsafe extern "system" fn(usize, usize, *mut c_void);
type MonoFieldGetValueObject = unsafe extern "system" fn(usize, usize, usize) -> usize;
type MonoFieldSetValue = unsafe extern "system" fn(usize, usize, *mut c_void);
type MonoClassVtable = unsafe extern "system" fn(usize, usize) -> usize;
type MonoStringToUtf8 = unsafe extern "system" fn(usize) -> *mut c_char;
type MonoFree = unsafe extern "system" fn(*mut c_void);
type MonoArrayLength = unsafe extern "system" fn(usize) -> usize;

const ARRAY_HEADER: usize = 32;

#[derive(Clone, Copy)]
pub struct MonoField {
    pub handle: usize,
    pub offset: u32,
}

impl MonoField {
    pub fn new(handle: usize, offset: u32) -> MonoField {
        MonoField { handle, offset }
    }
}

pub struct MonoHandler {
    domain: usize,
    threadAttach: MonoThreadAttach,
    imageLoaded: MonoImageLoaded,
    classFromName: MonoClassFromName,
    classGetFieldFromName: MonoClassGetFieldFromName,
    classGetMethodFromName: MonoClassGetMethodFromName,
    compileMethod: MonoCompileMethod,
    fieldGetOffset: MonoFieldGetOffset,
    fieldGetValue: MonoFieldGetValue,
    fieldGetValueObject: MonoFieldGetValueObject,
    fieldSetValue: MonoFieldSetValue,
    classVtable: MonoClassVtable,
    stringToUtf8: MonoStringToUtf8,
    free: MonoFree,
    arrayLength: MonoArrayLength,
}

impl MonoHandler {
    pub fn create() -> Result<MonoHandler, String> {
        let module = MONO_MODULES
            .iter()
            .find_map(|name| module_handle(name))
            .ok_or_else(|| "mono runtime not loaded".to_owned())?;
        let getRootDomain: MonoGetRootDomain =
            unsafe { std::mem::transmute(resolve(module, "mono_get_root_domain")?) };
        let domain = unsafe { getRootDomain() };
        if domain == 0 {
            return Err("mono domain unavailable".to_owned());
        }
        Ok(MonoHandler {
            domain,
            threadAttach: unsafe { std::mem::transmute(resolve(module, "mono_thread_attach")?) },
            imageLoaded: unsafe { std::mem::transmute(resolve(module, "mono_image_loaded")?) },
            classFromName: unsafe { std::mem::transmute(resolve(module, "mono_class_from_name")?) },
            classGetFieldFromName: unsafe {
                std::mem::transmute(resolve(module, "mono_class_get_field_from_name")?)
            },
            classGetMethodFromName: unsafe {
                std::mem::transmute(resolve(module, "mono_class_get_method_from_name")?)
            },
            compileMethod: unsafe {
                std::mem::transmute(resolve(module, "mono_compile_method")?)
            },
            fieldGetOffset: unsafe {
                std::mem::transmute(resolve(module, "mono_field_get_offset")?)
            },
            fieldGetValue: unsafe { std::mem::transmute(resolve(module, "mono_field_get_value")?) },
            fieldGetValueObject: unsafe {
                std::mem::transmute(resolve(module, "mono_field_get_value_object")?)
            },
            fieldSetValue: unsafe { std::mem::transmute(resolve(module, "mono_field_set_value")?) },
            classVtable: unsafe { std::mem::transmute(resolve(module, "mono_class_vtable")?) },
            stringToUtf8: unsafe { std::mem::transmute(resolve(module, "mono_string_to_utf8")?) },
            free: unsafe { std::mem::transmute(resolve(module, "mono_free")?) },
            arrayLength: unsafe { std::mem::transmute(resolve(module, "mono_array_length")?) },
        })
    }

    pub fn attach(&self) {
        unsafe {
            (self.threadAttach)(self.domain);
        }
    }

    pub fn image(&self, name: &str) -> Option<usize> {
        let name = CString::new(name).ok()?;
        let image = unsafe { (self.imageLoaded)(name.as_ptr()) };
        (image != 0).then_some(image)
    }

    pub fn class(&self, image: usize, namespace: &str, name: &str) -> Option<usize> {
        let namespace = CString::new(namespace).ok()?;
        let name = CString::new(name).ok()?;
        let class = unsafe { (self.classFromName)(image, namespace.as_ptr(), name.as_ptr()) };
        (class != 0).then_some(class)
    }

    pub fn vtable(&self, class: usize) -> Option<usize> {
        let vtable = unsafe { (self.classVtable)(self.domain, class) };
        (vtable != 0).then_some(vtable)
    }

    pub fn field(&self, class: usize, name: &str) -> Option<MonoField> {
        let name = CString::new(name).ok()?;
        let handle = unsafe { (self.classGetFieldFromName)(class, name.as_ptr()) };
        if handle == 0 {
            return None;
        }
        let offset = unsafe { (self.fieldGetOffset)(handle) };
        Some(MonoField::new(handle, offset))
    }

    pub fn method(&self, class: usize, name: &str, parameters: i32) -> Option<usize> {
        let name = CString::new(name).ok()?;
        let method = unsafe { (self.classGetMethodFromName)(class, name.as_ptr(), parameters) };
        (method != 0).then_some(method)
    }

    pub fn compiled(&self, class: usize, name: &str, parameters: i32) -> Option<usize> {
        let method = self.method(class, name, parameters)?;
        let code = unsafe { (self.compileMethod)(method) };
        (code != 0).then_some(code)
    }

    pub fn staticObject(&self, field: &MonoField) -> usize {
        unsafe { (self.fieldGetValueObject)(self.domain, field.handle, 0) }
    }

    pub fn fieldObject(&self, object: usize, field: &MonoField) -> usize {
        unsafe { (self.fieldGetValueObject)(self.domain, field.handle, object) }
    }

    pub fn fieldInt32(&self, object: usize, field: &MonoField) -> i32 {
        let mut value = 0i32;
        unsafe {
            (self.fieldGetValue)(object, field.handle, &mut value as *mut i32 as *mut c_void);
        }
        value
    }

    pub fn fieldFloat(&self, object: usize, field: &MonoField) -> f32 {
        let mut value = 0f32;
        unsafe {
            (self.fieldGetValue)(object, field.handle, &mut value as *mut f32 as *mut c_void);
        }
        value
    }

    pub fn setFieldInt32(&self, object: usize, field: &MonoField, value: i32) {
        let mut value = value;
        unsafe {
            (self.fieldSetValue)(object, field.handle, &mut value as *mut i32 as *mut c_void);
        }
    }

    pub fn setFieldFloat(&self, object: usize, field: &MonoField, value: f32) {
        let mut value = value;
        unsafe {
            (self.fieldSetValue)(object, field.handle, &mut value as *mut f32 as *mut c_void);
        }
    }

    pub fn text(&self, string: usize) -> String {
        if string == 0 {
            return String::new();
        }
        let pointer = unsafe { (self.stringToUtf8)(string) };
        if pointer.is_null() {
            return String::new();
        }
        let text = unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned();
        unsafe {
            (self.free)(pointer as *mut c_void);
        }
        text
    }

    pub fn arrayLength(&self, array: usize) -> usize {
        if array == 0 {
            return 0;
        }
        unsafe { (self.arrayLength)(array) }
    }

    pub fn arrayElement(&self, array: usize, index: usize) -> usize {
        MemoryUtility::read_value::<usize>(array + ARRAY_HEADER + index * 8).unwrap_or(0)
    }

    pub fn vtableOf(&self, object: usize) -> usize {
        if object == 0 {
            return 0;
        }
        MemoryUtility::read_value::<usize>(object).unwrap_or(0)
    }
}

fn resolve(module: HMODULE, name: &str) -> Result<usize, String> {
    let symbol = CString::new(name).map_err(|_| format!("{name} invalid"))?;
    let address = unsafe { GetProcAddress(module, symbol.as_ptr() as *const u8) };
    match address {
        Some(function) => Ok(function as usize),
        None => Err(format!("{name} missing")),
    }
}

