use crate::{
    error::IrpReaderError,
    snapshot::{Snapshot, TrackedVar, VarType},
};

pub struct SnapshotReader<'a> {
    snapshot: &'a Snapshot,
    tracked: &'a [TrackedVar],
}

impl<'a> SnapshotReader<'a> {
    pub fn new(snapshot: &'a Snapshot, tracked: &'a [TrackedVar]) -> Self {
        Self { snapshot, tracked }
    }

    fn find_var(&self, name: &str) -> Result<&TrackedVar, IrpReaderError> {
        self.tracked
            .iter()
            .find(|v| v.name == name)
            .ok_or_else(|| IrpReaderError::UnknownVariable(name.to_string()))
    }

    fn find_scalar(&self, name: &str) -> Result<&TrackedVar, IrpReaderError> {
        let var = self.find_var(name)?;
        if var.count != 1 {
            return Err(IrpReaderError::NotAScalar {
                name: name.to_string(),
                count: var.count,
            });
        }
        Ok(var)
    }

    fn expect_type(
        var: &TrackedVar,
        expected: VarType,
        label: &'static str,
    ) -> Result<(), IrpReaderError> {
        if std::mem::discriminant(&var.typ) != std::mem::discriminant(&expected) {
            return Err(IrpReaderError::TypeMismatch {
                name: var.name.to_string(),
                actual: var.typ,
                expected: label,
            });
        }
        Ok(())
    }

    fn scalar_bytes(&self, var: &TrackedVar) -> Result<&'a [u8], IrpReaderError> {
        let end = var.offset + var.typ.byte_size();
        self.snapshot
            .buf()
            .get(var.offset..end)
            .ok_or_else(|| IrpReaderError::OutOfBounds(var.name.to_string()))
    }

    pub fn get_bool(&self, name: &str) -> Result<bool, IrpReaderError> {
        let var = self.find_scalar(name)?;
        Self::expect_type(var, VarType::Bool, "a bool")?;
        let bytes = self.scalar_bytes(var)?;
        Ok(bytes[0] != 0)
    }

    pub fn get_float(&self, name: &str) -> Result<f32, IrpReaderError> {
        let var = self.find_scalar(name)?;
        Self::expect_type(var, VarType::Float, "a float")?;
        let bytes = self.scalar_bytes(var)?;
        Ok(f32::from_ne_bytes(bytes.try_into().unwrap()))
    }

    pub fn get_int(&self, name: &str) -> Result<i32, IrpReaderError> {
        let var = self.find_scalar(name)?;
        Self::expect_type(var, VarType::Int, "an int")?;
        let bytes = self.scalar_bytes(var)?;
        Ok(i32::from_ne_bytes(bytes.try_into().unwrap()))
    }

    pub fn get_float_array(&self, name: &str) -> Result<Vec<f32>, IrpReaderError> {
        let var = self.find_var(name)?;
        Self::expect_type(var, VarType::Float, "a float")?;

        Self::read_typed_array::<f32>(self.snapshot.buf(), var.offset, var.count)
            .ok_or_else(|| IrpReaderError::OutOfBounds(name.to_string()))
    }

    pub fn get_int_array(&self, name: &str) -> Result<Vec<i32>, IrpReaderError> {
        let var = self.find_var(name)?;
        Self::expect_type(var, VarType::Int, "an int")?;
        Self::read_typed_array::<i32>(self.snapshot.buf(), var.offset, var.count)
            .ok_or_else(|| IrpReaderError::OutOfBounds(name.to_string()))
    }

    fn read_typed_array<T: Copy>(buf: &[u8], offset: usize, count: usize) -> Option<Vec<T>> {
        let elem_size = size_of::<T>();
        let end = offset + elem_size * count;
        if end > buf.len() {
            return None;
        }
        let mut values = Vec::with_capacity(count);
        for i in 0..count {
            let elem_offset = offset + i * elem_size;
            let v = unsafe { (buf.as_ptr().add(elem_offset) as *const T).read_unaligned() };
            values.push(v);
        }
        Some(values)
    }
}
