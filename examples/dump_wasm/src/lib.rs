use shiguredo_mp4::{BaseBox, Decode, Mp4File, boxes::RootBox};

#[derive(Debug)]
struct BoxInfo {
    pub ty: String,
    pub unknown: Option<bool>,
    pub children: Vec<Self>,
}

impl BoxInfo {
    fn new(b: &dyn BaseBox) -> Self {
        Self {
            ty: b.box_type().to_string(),
            unknown: b.is_unknown_box().then_some(true),
            children: b.children().map(Self::new).collect(),
        }
    }
}

impl nojson::DisplayJson for BoxInfo {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            f.member("type", &self.ty)?;
            if let Some(unknown) = self.unknown {
                f.member("unknown", unknown)?;
            }
            if !self.children.is_empty() {
                f.member("children", &self.children)?;
            }
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
#[expect(clippy::not_unsafe_ptr_arg_deref)]
pub fn dump(bytes: *const u8, bytes_len: i32) -> *mut Vec<u8> {
    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len as usize) };

    let json = match Mp4File::<RootBox>::decode(bytes) {
        Ok((mp4, _)) => {
            let infos = mp4.iter().map(BoxInfo::new).collect::<Vec<_>>();
            nojson::json(|f| {
                f.set_indent_size(2);
                f.set_spacing(true);
                f.value(&infos)
            })
            .to_string()
        }
        Err(e) => e.to_string(),
    };

    Box::into_raw(Box::new(json.into_bytes()))
}

#[unsafe(no_mangle)]
#[expect(clippy::not_unsafe_ptr_arg_deref)]
pub fn vec_offset(v: *mut Vec<u8>) -> *mut u8 {
    unsafe { &mut *v }.as_mut_ptr()
}

#[unsafe(no_mangle)]
#[expect(clippy::not_unsafe_ptr_arg_deref)]
pub fn vec_len(v: *mut Vec<u8>) -> i32 {
    unsafe { &*v }.len() as i32
}

#[unsafe(no_mangle)]
pub fn allocate_vec(len: i32) -> *mut Vec<u8> {
    Box::into_raw(Box::new(vec![0; len as usize]))
}

#[unsafe(no_mangle)]
#[expect(clippy::not_unsafe_ptr_arg_deref)]
pub fn free_vec(v: *mut Vec<u8>) {
    let _ = unsafe { Box::from_raw(v) };
}
