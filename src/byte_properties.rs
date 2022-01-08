pub struct BytePropertiesFormatter<'a> {
    data: &'a [u8],
    line: usize,
}

impl<'a> BytePropertiesFormatter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, line: 0 }
    }

    pub fn reset(&mut self) {
        self.line = 0;
    }

    pub fn iter(&self) -> impl Iterator<Item = String> {
        let f_byte = if self.data.len() > 0 { self.data[0] } else { 0 };
        let data = vec![
            format!("hex: {:02x}", f_byte),
            format!("bin: {:08b}", f_byte),
        ];
        data.into_iter()
    }

    pub fn height() -> u16 {
        2
    }
}
