use floe_agent_contract::AgentFailure;

pub(crate) fn strict_json_bytes(data: &[u8], max: usize) -> Result<(), AgentFailure> {
    if data.len() > max {
        return Err(AgentFailure::InvalidInput);
    }
    let mut parser = JsonGuard { data, offset: 0 };
    parser.value(0)?;
    parser.whitespace();
    if parser.offset != data.len() {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

struct JsonGuard<'data> {
    data: &'data [u8],
    offset: usize,
}

impl JsonGuard<'_> {
    fn whitespace(&mut self) {
        while self
            .data
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.offset += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<(), AgentFailure> {
        if depth > 32 {
            return Err(AgentFailure::InvalidInput);
        }
        self.whitespace();
        match self.data.get(self.offset).copied() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => self.string().map(|_| ()),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'n') => self.literal(b"null"),
            _ => Err(AgentFailure::InvalidInput),
        }
    }

    fn object(&mut self, depth: usize) -> Result<(), AgentFailure> {
        self.offset += 1;
        self.whitespace();
        let mut keys = Vec::new();
        if self.data.get(self.offset) == Some(&b'}') {
            self.offset += 1;
            return Ok(());
        }
        loop {
            self.whitespace();
            let key = self.string()?;
            if keys.iter().any(|candidate: &String| candidate == &key) {
                return Err(AgentFailure::InvalidInput);
            }
            keys.push(key);
            self.whitespace();
            if self.data.get(self.offset) != Some(&b':') {
                return Err(AgentFailure::InvalidInput);
            }
            self.offset += 1;
            self.value(depth)?;
            self.whitespace();
            match self.data.get(self.offset) {
                Some(b',') => self.offset += 1,
                Some(b'}') => {
                    self.offset += 1;
                    return Ok(());
                }
                _ => return Err(AgentFailure::InvalidInput),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<(), AgentFailure> {
        self.offset += 1;
        self.whitespace();
        if self.data.get(self.offset) == Some(&b']') {
            self.offset += 1;
            return Ok(());
        }
        loop {
            self.value(depth)?;
            self.whitespace();
            match self.data.get(self.offset) {
                Some(b',') => self.offset += 1,
                Some(b']') => {
                    self.offset += 1;
                    return Ok(());
                }
                _ => return Err(AgentFailure::InvalidInput),
            }
        }
    }

    fn string(&mut self) -> Result<String, AgentFailure> {
        let start = self.offset;
        if self.data.get(self.offset) != Some(&b'"') {
            return Err(AgentFailure::InvalidInput);
        }
        self.offset += 1;
        while let Some(byte) = self.data.get(self.offset).copied() {
            self.offset += 1;
            match byte {
                b'"' => {
                    return serde_json::from_slice(&self.data[start..self.offset])
                        .map_err(|_| AgentFailure::InvalidInput);
                }
                b'\\' => {
                    self.offset += 1;
                    if self.data.get(self.offset - 1).is_none() {
                        return Err(AgentFailure::InvalidInput);
                    }
                }
                0..=0x1f => return Err(AgentFailure::InvalidInput),
                _ => {}
            }
        }
        Err(AgentFailure::InvalidInput)
    }

    fn number(&mut self) -> Result<(), AgentFailure> {
        let start = self.offset;
        while self
            .data
            .get(self.offset)
            .is_some_and(|byte| matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.offset += 1;
        }
        if start == self.offset {
            Err(AgentFailure::InvalidInput)
        } else {
            Ok(())
        }
    }

    fn literal(&mut self, literal: &[u8]) -> Result<(), AgentFailure> {
        if self.data.get(self.offset..self.offset + literal.len()) == Some(literal) {
            self.offset += literal.len();
            Ok(())
        } else {
            Err(AgentFailure::InvalidInput)
        }
    }
}
