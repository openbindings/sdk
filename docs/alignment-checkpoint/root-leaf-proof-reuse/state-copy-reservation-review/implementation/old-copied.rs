    fn copied(&self, graph: &mut Graph<'_>) -> Result<Self, NoVerdict> {
        debug_assert!(self.witness.is_none() || self.valid == FALSE);
        graph.work(
            self.items
                .len()
                .saturating_add(self.properties.len())
                .saturating_add(1),
        )?;
        let mut bytes = 256usize.saturating_add(self.items.len().saturating_mul(128));
        for name in self.properties.keys() {
            graph.work(name.len().saturating_add(1))?;
            bytes = bytes.saturating_add(name.len().saturating_mul(2).saturating_add(256));
        }
        graph.charge(bytes)?;
        Ok(self.clone())
    }
