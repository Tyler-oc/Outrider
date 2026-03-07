use ort::{inputs, session::Session, value::Tensor};
use tokenizers::{Tokenizer, TruncationDirection, TruncationParams, TruncationStrategy};

pub type EmbedResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct EmbeddingModel {
    session: Session,
    tokenizer: Tokenizer,
}

impl EmbeddingModel {
    pub fn load() -> EmbedResult<Self> {
        let model_path =
            std::env::var("MODEL_PATH").unwrap_or_else(|_| "models/model.onnx".to_string());
        let tokenizer_path = std::env::var("TOKENIZER_PATH")
            .unwrap_or_else(|_| "models/tokenizer.json".to_string());

        // Returns bool in this RC version, not Result — no ? needed.
        ort::init().with_name("backend").commit();
        let session = Session::builder()?.commit_from_file(&model_path)?;

        let mut tokenizer = Tokenizer::from_file(&tokenizer_path)?;

        // Truncate at 256 tokens — the sequence length all-MiniLM-L6-v2 was
        // trained on. Longer input is silently right-truncated.
        tokenizer.with_truncation(Some(TruncationParams {
            direction: TruncationDirection::Right,
            max_length: 256,
            strategy: TruncationStrategy::LongestFirst,
            stride: 0,
        }))?;

        Ok(Self { session, tokenizer })
    }

    pub fn embed(&mut self, text: &str) -> EmbedResult<Vec<f32>> {
        // --- tokenise ---
        let encoding = self.tokenizer.encode(text, /* add_special_tokens */ true)?;

        let seq_len = encoding.get_ids().len();

        let input_ids: Box<[i64]> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let attn_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();
        let type_ids: Box<[i64]> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();

        let ids_val = Tensor::<i64>::from_array(([1usize, seq_len], input_ids))?;
        let mask_val = Tensor::<i64>::from_array((
            [1usize, seq_len],
            attn_mask.iter().copied().collect::<Box<[_]>>(),
        ))?;
        let type_val = Tensor::<i64>::from_array(([1usize, seq_len], type_ids))?;

        // --- ONNX inference ---
        // inputs! returns Vec<...> directly — no ? on the macro.
        let outputs = self.session.run(inputs![
            "input_ids"      => ids_val,
            "attention_mask" => mask_val,
            "token_type_ids" => type_val,
        ])?;

        // try_extract_array (ndarray feature) → ArrayViewD<f32>, shape [1, seq_len, 384].
        let hidden = outputs["last_hidden_state"].try_extract_array::<f32>()?;

        // --- attention-masked mean pooling ---
        let mask_f: Vec<f32> = attn_mask.iter().map(|&x| x as f32).collect();
        let mask_sum: f32 = mask_f.iter().sum();

        let mut pooled = vec![0.0f32; 384];
        for (token_idx, &weight) in mask_f.iter().enumerate() {
            for dim in 0..384 {
                pooled[dim] += hidden[[0, token_idx, dim]] * weight;
            }
        }
        for v in &mut pooled {
            *v /= mask_sum;
        }

        // --- L2 normalisation ---
        let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-12 {
            for v in &mut pooled {
                *v /= norm;
            }
        }

        Ok(pooled)
    }
}
