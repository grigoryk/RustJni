extern crate libc;

use std::mem;
use std::fmt;
use std::string;
use std::ffi::CString;

use native::*;

#[derive(Debug, Clone)]
pub struct JavaVMOption {
	pub optionString: string::String,
	pub extraInfo: *const ::libc::c_void
}

impl JavaVMOption {
	pub fn new(option: &str, extra: *const ::libc::c_void) -> JavaVMOption {
		JavaVMOption{
			optionString: option.to_string(),
			extraInfo: extra
		}
	}
}

#[derive(Debug)]
pub struct JavaVMInitArgs {
	pub version: JniVersion,
	pub options: Vec<JavaVMOption>,
	pub ignoreUnrecognized: bool
}

impl JavaVMInitArgs {
	pub fn new(version: JniVersion, options: &[JavaVMOption], ignoreUnrecognized: bool) -> JavaVMInitArgs {
		JavaVMInitArgs{
			version: version,
			options: options.to_vec(),
			ignoreUnrecognized: ignoreUnrecognized
		}
	}
}


#[derive(Debug, Clone)]
pub struct JavaVMAttachArgs {
	pub version: JniVersion,
	pub name: string::String,
	pub group: JavaObject
}

impl JavaVMAttachArgs {
	pub fn new(version: JniVersion, name: &str, group: JavaObject) -> JavaVMAttachArgs {
		JavaVMAttachArgs{
			version: version,
			name: name.to_string(),
			group: group
		}
	}
}


#[derive(Debug)]
pub struct JavaVM {
	ptr: *mut JavaVMImpl,
	version: JniVersion,
	name: Box<String>,
}

impl JavaVM {
	pub fn new(args: JavaVMInitArgs, name: &str) -> JavaVM {
		use std::borrow::ToOwned;
		let (res, jvm) = unsafe {
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
			let mut vm_opts = vec![];
			let mut vm_opts_vect = vec![];
			for opt in args.options.iter() {
				let cstr:CString = CString::new(&opt.optionString[..]).unwrap();
				vm_opts.push(
					JavaVMOptionImpl {
						optionString: cstr.as_ptr(),
						extraInfo: opt.extraInfo
					});
				vm_opts_vect.push(cstr);
			}

			let mut argsImpl = JavaVMInitArgsImpl{
				version: args.version,
				nOptions: args.options.len() as jint,
				options: vm_opts.as_mut_ptr(),
				ignoreUnrecognized: args.ignoreUnrecognized as jboolean
			};

			let res = JNI_CreateJavaVM(&mut jvm, &mut env, &mut argsImpl);

			(res, jvm)
		};

		match res {
			JniError::JNI_OK => JavaVM{
				ptr: jvm,
				version: args.version,
				name: Box::new(name[..].to_owned())
			},
			_ => panic!("JNI_CreateJavaVM error: {:?}", res)
		}
	}
/*
	pub fn from(ptr: *mut JavaVMImpl) -> JavaVM {
		let mut res = JavaVM{
			ptr: ptr,
			version: JniVersion::JNI_VERSION_1_1,
			name: Box::<String>::new(String.new()),
		};
		res.version = res.get_env().version();
		res
	}
*/
	pub fn ptr(&self) -> *mut JavaVMImpl {
		self.ptr
	}

	pub fn version(&self) -> JniVersion {
		return self.version
	}

	pub fn get_env(&mut self) -> JavaEnv {
		unsafe {
			let jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThread)
		}
	}

	pub fn get_env_daemon(&mut self) -> JavaEnv {
		unsafe {
			let jni = **self.ptr;
			self.get_env_gen(jni.AttachCurrentThreadAsDaemon)
		}
	}

	pub fn detach_current_thread(&mut self) -> bool {
		unsafe {
			let jni = **self.ptr;
			(jni.DetachCurrentThread)(self.ptr) == JniError::JNI_OK
		}
	}

	unsafe fn get_env_gen(&mut self, fun: extern "C" fn(vm: *mut JavaVMImpl, penv: &mut *mut JNIEnvImpl, args: *mut JavaVMAttachArgsImpl) -> JniError) -> JavaEnv {
		let mut env: *mut JNIEnvImpl = 0 as *mut JNIEnvImpl;
		let res = ((**self.ptr).GetEnv)(self.ptr, &mut env, self.version());
		match res {
			JniError::JNI_OK => JavaEnv {ptr: env},
			JniError::JNI_EDETACHED => {
				let mut attachArgs = JavaVMAttachArgsImpl{
					version: self.version(),
					name: self.name.as_ptr() as *const libc::c_char,
					group: 0 as jobject
				};
				let res = fun(self.ptr, &mut env, &mut attachArgs);
				match res {
					JniError::JNI_OK => JavaEnv {ptr: env},
					_ => panic!("AttachCurrentThread error {:?}!", res)
				}
			},
			JniError::JNI_EVERSION => panic!("Version {:?} is not supported by GetEnv!", self.version()),
			_ => panic!("GetEnv error {:?}!", res)
		}
	}
	
	unsafe fn destroy_java_vm(&self) -> bool {
		((**self.ptr).DestroyJavaVM)(self.ptr) == JniError::JNI_OK
	}
}

impl Drop for JavaVM {
	fn drop(&mut self) {
		unsafe {
			self.destroy_java_vm();
		}
	}
}

#[derive(Debug, Clone)]
pub struct JavaEnv {
	ptr: *mut JNIEnvImpl
}

impl Copy for JavaEnv {}

impl JavaEnv {
	pub fn version(&self) -> JniVersion {
		unsafe {
			mem::transmute(((**self.ptr).GetVersion)(self.ptr))
		}
	}

	pub fn ptr(&self) -> *mut JNIEnvImpl {
		self.ptr
	}

	pub fn define_class<T: JObject>(&self, name: &JavaChars, loader: &T, buf: &[u8], len: usize) -> JavaClass {
		JObject::from(
			self,
			unsafe { ((**self.ptr).DefineClass)(
				self.ptr,
				name.as_ptr() as *const ::libc::c_char,
				loader.get_obj(),
				buf.as_ptr() as *const jbyte,
				len as jsize
			) } as jobject
		)
	}

	// Takes a string and returns a Java class if successfull.
	// Returns `None` on failure.
	pub fn find_class(&self, name: &JavaChars) -> Option<JavaClass> {
		let ptr = unsafe { ((**self.ptr).FindClass)(
			self.ptr, name.as_ptr()) };
		if ptr == (0 as jclass) {
			None
		} else {
			Some(JObject::from(self, ptr as jobject))
		}
	}

	pub fn get_super_class(&self, sub: &JavaClass) -> JavaClass {
		JObject::from( self, unsafe {
			((**self.ptr).GetSuperclass)(self.ptr, sub.ptr) as jobject
		})
	}

	pub fn is_assignable_from(&self, sub: &JavaClass, sup: &JavaClass) -> bool {
		unsafe {
			((**self.ptr).IsAssignableFrom)(self.ptr, sub.ptr, sup.ptr) != 0
		}
	}


	pub fn throw(&self, obj: &JavaThrowable) -> bool {
		unsafe {
			((**self.ptr).Throw)(self.ptr, obj.ptr) == JniError::JNI_OK
		}
	}

	pub fn throw_new(&self, clazz: &JavaClass, msg: &JavaChars) -> bool {
		unsafe {
			((**self.ptr).ThrowNew)(self.ptr, clazz.ptr, msg.as_ptr() as *const ::libc::c_char) == JniError::JNI_OK
		}
	}

	pub fn exception_occured(&self) -> JavaThrowable {
		JObject::from(
			self,
			unsafe {
				((**self.ptr).ExceptionOccurred)(self.ptr) as jobject
			}
		)
	}

	pub fn exception_describe(&self) {
		unsafe {
			((**self.ptr).ExceptionDescribe)(self.ptr)
		}
	}

	pub fn exception_clear(&self) {
		unsafe {
			((**self.ptr).ExceptionClear)(self.ptr)
		}
	}

	pub fn fatal_error(&self, msg: &JavaChars) {
		unsafe {
			((**self.ptr).FatalError)(self.ptr, msg.as_ptr())
		}
	}

	pub fn push_local_frame(&self, capacity: isize) -> bool {
		unsafe {
			((**self.ptr).PushLocalFrame)(self.ptr, capacity as jint) == JniError::JNI_OK
		}
	}

	pub fn pop_local_frame<T: JObject>(&self, result: &T) -> T {
		JObject::from(self, unsafe {
			((**self.ptr).PopLocalFrame)(self.ptr, result.get_obj())
		})
	}

	pub fn is_same_object<T1: JObject, T2: JObject>(&self, obj1: &T1, obj2: &T2) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), obj2.get_obj()) != 0
		}
	}

	pub fn is_null<T: JObject>(&self, obj1: &T) -> bool {
		unsafe {
			((**self.ptr).IsSameObject)(self.ptr, obj1.get_obj(), 0 as jobject) != 0
		}
	}

	fn new_local_ref<T: JObject>(&self, lobj: &T) -> jobject {
		unsafe {
			((**self.ptr).NewLocalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_local_ref<T: JObject>(&self, gobj: &T) {
		unsafe {
			((**self.ptr).DeleteLocalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_global_ref<T: JObject>(&self, lobj: &T) -> jobject {
		unsafe {
			((**self.ptr).NewGlobalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_global_ref<T: JObject>(&self, gobj: &T) {
		unsafe {
			((**self.ptr).DeleteGlobalRef)(self.ptr, gobj.get_obj())
		}
	}

	fn new_weak_ref<T: JObject>(&self, lobj: &T) -> jweak {
		unsafe {
			((**self.ptr).NewWeakGlobalRef)(self.ptr, lobj.get_obj())
		}
	}

	fn delete_weak_ref<T: JObject>(&self, wobj: &T) {
		unsafe {
			((**self.ptr).DeleteWeakGlobalRef)(self.ptr, wobj.get_obj() as jweak)
		}
	}

	pub fn ensure_local_capacity(&self, capacity: isize) -> bool {
		unsafe {
			((**self.ptr).EnsureLocalCapacity)(self.ptr, capacity as jint) == JniError::JNI_OK
		}
	}

	pub fn alloc_object(&self, clazz: &JavaClass) -> JavaObject {
		JObject::from(self, unsafe {
			((**self.ptr).AllocObject)(self.ptr, clazz.ptr)
		})
	}

	pub fn monitor_enter<T: JObject>(&self, obj: &T) -> bool {
		unsafe {
			((**self.ptr).MonitorEnter)(self.ptr, obj.get_obj()) == JniError::JNI_OK
		}
	}

	pub fn monitor_exit<T: JObject>(&self, obj: &T) -> bool {
		unsafe {
			((**self.ptr).MonitorExit)(self.ptr, obj.get_obj()) == JniError::JNI_OK
		}
	}
/*
	pub fn jvm(&self) -> &mut JavaVM {
		JavaVM::from(unsafe {
			let mut jvm: *mut JavaVMImpl = 0 as *mut JavaVMImpl;
			((**self.ptr).GetJavaVM)(self.ptr, &mut jvm);
			jvm
		})
	}
*/
	pub fn exception_check(&self) -> bool {
		unsafe {
			((**self.ptr).ExceptionCheck)(self.ptr) != 0
		}
	}
}

#[derive(Debug, Clone)]
enum RefType {
	Local,
	Global,
	Weak
}

impl Copy for RefType {}

pub trait JObject: Drop + Clone {
	fn get_env(&self) -> JavaEnv;
	fn get_obj(&self) -> jobject;
	fn ref_type(&self) -> RefType;

	fn from(env: &JavaEnv, ptr: jobject) -> Self;
	fn global(&self) -> Self;
	fn weak(&self) -> Self;

	fn inc_ref(&self) -> jobject {
		let env = self.get_env();
		match self.ref_type() {
			RefType::Local => env.new_local_ref(self),
			RefType::Global => env.new_global_ref(self),
			RefType::Weak => env.new_weak_ref(self) as jobject,
		}
	}

	fn dec_ref(&mut self) {
		let env = self.get_env();
		match self.ref_type() {
			RefType::Local => env.delete_local_ref(self),
			RefType::Global => env.delete_global_ref(self),
			RefType::Weak => env.delete_weak_ref(self)
		}
	}

	fn get_class(&self) -> JavaClass {
		let env = self.get_env();
		JObject::from(&env, unsafe {
			((**env.ptr).GetObjectClass)(env.ptr, self.get_obj()) as jobject
		})
	}

	fn as_jobject(&self) -> JavaObject {
		JavaObject{
			env: self.get_env(),
			ptr: self.inc_ref(),
			rtype: self.ref_type()
		}
	}

	fn is_instance_of(&self, clazz: &JavaClass) -> bool {
		let env = self.get_env();
		unsafe {
			((**env.ptr).IsInstanceOf)(env.ptr, self.get_obj(), clazz.ptr) != 0
		}
	}

	fn is_same<T: JObject>(&self, val: &T) -> bool {
		self.get_env().is_same_object(self, val)
	}

	fn is_null(&self) -> bool {
		self.get_env().is_null(self)
	}
}

pub trait JArray: JObject {}


macro_rules! impl_jobject_base(
	($cls:ident) => (
		impl Drop for $cls {
			fn drop(&mut self) {
				self.dec_ref();
			}
		}

		impl Clone for $cls {
			fn clone(&self) -> $cls {
				$cls {
					env: self.get_env(),
					ptr: self.inc_ref(),
					rtype: self.rtype
				}
			}
		}
	);
);

macro_rules! impl_jobject(
	($cls:ident, $native:ident) => (
		impl_jobject_base!($cls);

		impl JObject for $cls {

			fn get_env(&self) -> JavaEnv {
				self.env
			}

			fn get_obj(&self) -> jobject {
				self.ptr as jobject
			}

			fn ref_type(&self) -> RefType {
				self.rtype
			}

			fn from(env: &JavaEnv, ptr: jobject) -> $cls {
				$cls{
					env: env.clone(),
					ptr: ptr as $native,
					rtype: RefType::Local
				}
			}

			fn global(&self) -> $cls {
				let env = self.get_env();
				$cls{
					env: env,
					ptr: env.new_global_ref(self),
					rtype: RefType::Global
				}
			}

			fn weak(&self) -> $cls {
				let env = self.get_env();
				$cls{
					env: env,
					ptr: env.new_weak_ref(self),
					rtype: RefType::Weak
				}
			}
		}
	);
);

macro_rules! impl_jarray(
	($cls:ident, $native:ident) => (
		impl_jobject!($cls, $native);

		// impl $cls {
		//		pub fn as_jarray(&self) -> JavaArray {
		//			self.inc_ref();
		//			JavaArray {
		//				env: self.get_env(),
		//				ptr: self.ptr as jarray
		//			}
		//		}
		// }
	);
);



#[derive(Debug)]
pub struct JavaObject {
	env: JavaEnv,
	ptr: jobject,
	rtype: RefType
}

impl_jobject!(JavaObject, jobject);


#[derive(Debug)]
pub struct JavaClass {
	env: JavaEnv,
	ptr: jclass,
	rtype: RefType
}

impl_jobject!(JavaClass, jclass);

impl JavaClass {
	pub fn get_super(&self) -> JavaClass {
		self.get_env().get_super_class(self)
	}

	pub fn alloc(&self) -> JavaObject {
		self.get_env().alloc_object(self)
	}

	pub fn find(env: &JavaEnv, name: &JavaChars) -> JavaClass {
		match env.find_class(name) {
			None => panic!("Class {:?} not found!", name),
			Some(cls) => cls
		}
	}
}


#[derive(Debug)]
pub struct JavaThrowable {
	env: JavaEnv,
	ptr: jthrowable,
	rtype: RefType
}

impl_jobject!(JavaThrowable, jthrowable);

#[derive(Debug)]
pub struct JavaString {
	env: JavaEnv,
	ptr: jstring,
	rtype: RefType
}

impl_jobject!(JavaString, jstring);

use super::j_chars::JavaChars;
impl JavaString {
	pub fn new(env: JavaEnv, val: &super::j_chars::JavaChars) -> JavaString {
		JObject::from(&env, unsafe {
			((**env.ptr).NewStringUTF)(env.ptr, val.as_ptr()) as jobject
		})
	}

	pub fn len(&self) -> usize {
		unsafe {
			((**self.get_env().ptr).GetStringLength)(self.get_env().ptr, self.ptr) as usize
		}
	}

	pub fn size(&self) -> usize {
		unsafe {
			((**self.get_env().ptr).GetStringUTFLength)(self.get_env().ptr, self.ptr) as usize
		}
	}

	pub fn to_str(&self) -> string::String {
		self.chars().to_str()
	}

	fn chars(&self) -> JavaStringChars {
		let mut isCopy: jboolean = 0;
		let val = unsafe {
			((**self.get_env().ptr).GetStringUTFChars)(self.get_env().ptr, self.ptr, &mut isCopy)
		};
		JavaStringChars{
			s: self.clone(),
			chars: val
		}
	}

	pub fn region(&self, start: usize, length: usize) -> string::String {
		let size = self.size();
		let mut vec: Vec<u8> = Vec::with_capacity(size);
		unsafe {
			((**self.get_env().ptr).GetStringUTFRegion)(self.get_env().ptr, self.ptr, start as jsize, length as jsize, vec.as_mut_ptr() as *mut ::libc::c_char);
			vec.set_len(length as usize);
		}

		match string::String::from_utf8(vec) {
			Ok(res) => res,
			Err(_) => "".to_string()
		}
	}
}


struct JavaStringChars {
	s: JavaString,
	chars: *const ::libc::c_char
}

impl Drop for JavaStringChars {
  fn drop(&mut self) {
		unsafe {
			((**self.s.env.ptr).ReleaseStringUTFChars)(self.s.env.ptr, self.s.ptr, self.chars)
		}
	}
}


impl fmt::Debug for JavaStringChars {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "\"{}\"", self.to_str())
	}
}

impl JavaStringChars {
	fn to_str(&self) -> string::String {
		unsafe {
			string::String::from_str(
				::std::str::from_utf8_unchecked(
					::std::ffi::CStr::from_ptr(self.chars).to_bytes()
				)
			)
		}
	}
}

/*
// For future
trait JavaPrimitive {}

impl JavaPrimitive for jboolean {}
impl JavaPrimitive for jbyte {}
impl JavaPrimitive for jchar {}
impl JavaPrimitive for jshort {}
impl JavaPrimitive for jint {}
impl JavaPrimitive for jlong {}
impl JavaPrimitive for jfloat {}
impl JavaPrimitive for jdouble {}
// impl JavaPrimitive for jsize {}
*/
use ::std::marker::PhantomData;
pub struct JavaArray<T> {
	env: JavaEnv,
	ptr: jarray,
	rtype: RefType,
	phantom: PhantomData<T>,
}

#[unsafe_destructor]
impl<T> Drop for JavaArray<T> {
	fn drop(&mut self) {
		self.dec_ref();
	}
}

impl<T> Clone for JavaArray<T> {
	fn clone(&self) -> JavaArray<T> {
		JavaArray{
			env: self.get_env(),
			ptr: self.inc_ref(),
			rtype: self.rtype,
			phantom: PhantomData::<T>,
		}
	}
}

impl<T> JObject for JavaArray<T> {
	fn get_env(&self) -> JavaEnv {
		self.env
	}

	fn get_obj(&self) -> jobject {
		self.ptr as jobject
	}

	fn ref_type(&self) -> RefType {
		self.rtype
	}

	fn from(env: &JavaEnv, ptr: jobject) -> JavaArray<T> {
		JavaArray{
			env: env.clone(),
			ptr: ptr as jarray,
			rtype: RefType::Local,
			phantom: PhantomData::<T>,
		}
	}

	fn global(&self) -> JavaArray<T> {
		let env = self.get_env();
		JavaArray{
			env: env,
			ptr: env.new_global_ref(self),
			rtype: RefType::Global,
			phantom: PhantomData::<T>,
		}
	}

	fn weak(&self) -> JavaArray<T> {
		let env = self.get_env();
		JavaArray{
			env: env,
			ptr: env.new_weak_ref(self),
			rtype: RefType::Weak,
			phantom: PhantomData::<T>,
		}
	}
}
/*

unsafe fn JavaVMOptionImpl_new(opt: &::jni::JavaVMOption) -> JavaVMOptionImpl {
	let cstring = CString::unchecked_from_bytes(opt.optionString[..].as_bytes());
	JavaVMOptionImpl{
		optionString: cstring.as_ptr(),// opt.optionString[..].as_ptr() as * const ::libc::c_char, // TOSO: remove odd cast
		extraInfo: opt.extraInfo
	}
}

*/
// vim: set noexpandtab:
// vim: set tabstop=4:
// vim: set shiftwidth=4:
// Local Variables:
// mode: rust
// indent-tabs-mode: t
// rust-indent-offset: 4
// tab-width: 4
// End:
