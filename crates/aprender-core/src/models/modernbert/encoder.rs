//! The ModernBERT encoder forward: embeddings -> N layers -> final LayerNorm, with a
//! ladder tap at every block (`emb`, `layer{i}`, `final`) for parity tests.

use super::{layer_norm, ModernBertConfig, ModernBertEmbeddings, ModernBertError, ModernBertLayer};

/// A loaded ModernBERT encoder. Build it with [`ModernBertEncoder::from_apr`].
#[derive(Debug, Clone)]
pub struct ModernBertEncoder {
    config: ModernBertConfig,
    embeddings: ModernBertEmbeddings,
    layers: Vec<ModernBertLayer>,
    final_norm: Vec<f32>,
    /// Test-only local half-window override for the `window_mutation` rung.
    #[cfg(test)]
    window_override: Option<usize>,
}

impl ModernBertEncoder {
    pub(crate) fn from_parts(
        config: ModernBertConfig,
        embeddings: ModernBertEmbeddings,
        layers: Vec<ModernBertLayer>,
        final_norm: Vec<f32>,
    ) -> Self {
        Self {
            config,
            embeddings,
            layers,
            final_norm,
            #[cfg(test)]
            window_override: None,
        }
    }

    /// The validated config this encoder was loaded with.
    pub fn config(&self) -> &ModernBertConfig {
        &self.config
    }

    /// The encoder layers, in order.
    pub fn layers(&self) -> &[ModernBertLayer] {
        &self.layers
    }

    /// Test-only: replace the local half-window (the `window_mutation` rung).
    #[cfg(test)]
    pub(crate) fn set_window_override(&mut self, window: Option<usize>) {
        self.window_override = window;
    }

    fn window(&self) -> usize {
        #[cfg(test)]
        if let Some(w) = self.window_override {
            return w;
        }
        self.config.window()
    }

    /// Encode one unpadded row. Returns the final-norm output `[ids.len(), d]`;
    /// `tap(name, block)` sees `emb`, `layer{i}` and `final`.
    ///
    /// # Errors
    ///
    /// [`ModernBertError::EmptyInput`] for an empty row,
    /// [`ModernBertError::OutOfVocab`] for an id outside the vocabulary, or a shape /
    /// GEMM error from the primitives.
    #[provable_contracts_macros::contract("laya-parity-v1", equation = "per_layer_rel_rms")]
    pub fn forward(
        &self,
        ids: &[u32],
        mut tap: impl FnMut(&str, &[f32]),
    ) -> Result<Vec<f32>, ModernBertError> {
        if ids.is_empty() {
            return Err(ModernBertError::EmptyInput);
        }
        let (d, l) = (self.config.hidden_size(), ids.len());
        let eps = self.config.norm_eps();
        let window = self.window();
        let mut x = self.embeddings.forward(ids, eps)?;
        tap("emb", &x);
        for (li, layer) in self.layers.iter().enumerate() {
            layer.forward(&mut x, l, &self.config, window)?;
            tap(&format!("layer{li}"), &x);
        }
        let x = layer_norm(&x, d, &self.final_norm, None, eps)?;
        tap("final", &x);
        Ok(x)
    }
}
