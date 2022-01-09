pub struct BytePropertiesFormatter<'a> {
    data: &'a [u8],
}

impl<'a> BytePropertiesFormatter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    pub fn iter(&self) -> impl Iterator<Item = String> {
        let f_byte = if !self.data.is_empty() {
            self.data[0]
        } else {
            0
        };
        let data = vec![
            format!("hex: {:02x}", f_byte),
            format!("bin: {:08b}", f_byte),
            format!("dec: {}", f_byte),
            format!("oct: {:o}", f_byte),
        ];
        data.into_iter()
    }

    pub fn height() -> u16 {
        4
    }
}
