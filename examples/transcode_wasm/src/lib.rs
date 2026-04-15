use std::fmt;

pub mod mp4;
pub mod transcode;
pub mod wasm;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Error {
    pub message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl nojson::DisplayJson for Error {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> fmt::Result {
        f.object(|f| f.member("message", &self.message))
    }
}

impl<'text, 'raw> TryFrom<nojson::RawJsonValue<'text, 'raw>> for Error {
    type Error = nojson::JsonParseError;

    fn try_from(
        value: nojson::RawJsonValue<'text, 'raw>,
    ) -> std::result::Result<Self, Self::Error> {
        let message = value.to_member("message")?.required()?.try_into()?;
        Ok(Self { message })
    }
}

/// JS との境界で `{"Ok":..}` / `{"Err":..}` 形式を扱うラッパー
pub struct JsonResult<T>(pub Result<T>);

impl<T: nojson::DisplayJson> nojson::DisplayJson for JsonResult<T> {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> fmt::Result {
        match &self.0 {
            Ok(value) => f.object(|f| f.member("Ok", value)),
            Err(err) => f.object(|f| f.member("Err", err)),
        }
    }
}

impl<'text, 'raw, T> TryFrom<nojson::RawJsonValue<'text, 'raw>> for JsonResult<T>
where
    T: TryFrom<nojson::RawJsonValue<'text, 'raw>, Error = nojson::JsonParseError>,
{
    type Error = nojson::JsonParseError;

    fn try_from(
        value: nojson::RawJsonValue<'text, 'raw>,
    ) -> std::result::Result<Self, Self::Error> {
        if let Some(ok) = value.to_member("Ok")?.optional() {
            Ok(Self(Ok(T::try_from(ok)?)))
        } else {
            let err = value.to_member("Err")?.required()?.try_into()?;
            Ok(Self(Err(err)))
        }
    }
}
