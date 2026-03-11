use anyhow::Result;
use sha2::Digest;
#[cfg(feature = "gpu-embedded")]
use std::borrow::Cow;
use std::fmt;

/// Represents the active compute backend for transparency in stats
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendType {
    CpuFallback,
    GpuVulkan,
    GpuMetal,
    GpuExternal,
}

impl fmt::Display for BackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendType::CpuFallback => write!(f, "cpu_fallback"),
            BackendType::GpuVulkan => write!(f, "gpu_vulkan"),
            BackendType::GpuMetal => write!(f, "gpu_metal"),
            BackendType::GpuExternal => write!(f, "gpu_external"),
        }
    }
}

pub enum ComputeBackend {
    #[cfg(feature = "gpu-embedded")]
    Embedded {
        device: wgpu::Device,
        queue: wgpu::Queue,
        compute_pipeline: wgpu::ComputePipeline,
        backend_type: BackendType,
    },
    External {
        worker: std::sync::Mutex<std::process::Child>,
        backend_type: BackendType,
    },
}

impl ComputeBackend {
    pub fn backend_type(&self) -> BackendType {
        match self {
            #[cfg(feature = "gpu-embedded")]
            Self::Embedded { backend_type, .. } => backend_type.clone(),
            Self::External { backend_type, .. } => backend_type.clone(),
        }
    }

    fn try_spawn_external() -> Option<Self> {
        // Try to find `curd-gpu-worker`
        let exe_path = std::env::current_exe().ok()?;
        let exe_dir = exe_path.parent()?;
        let worker_path = exe_dir.join("curd-gpu-worker");

        let path_to_try = if worker_path.exists() {
            worker_path.to_string_lossy().to_string()
        } else {
            "curd-gpu-worker".to_string()
        };

        if let Ok(child) = std::process::Command::new(&path_to_try)
            .env_remove("CURD_FORCE_EXTERNAL_GPU")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
        {
            // Successfully spawned
            return Some(Self::External {
                worker: std::sync::Mutex::new(child),
                backend_type: BackendType::GpuExternal,
            });
        }
        None
    }

    #[cfg(not(feature = "gpu-embedded"))]
    pub fn new() -> Result<Option<Self>> {
        if let Some(ext) = Self::try_spawn_external() {
            return Ok(Some(ext));
        }
        Ok(None)
    }

    #[cfg(feature = "gpu-embedded")]
    pub fn new() -> Result<Option<Self>> {
        if std::env::var("CURD_FORCE_EXTERNAL_GPU").is_ok()
            && let Some(ext) = Self::try_spawn_external()
        {
            return Ok(Some(ext));
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL,
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }));

        let Some(adapter) = adapter else {
            return Ok(None);
        };

        let backend_info = adapter.get_info().backend;
        let backend_type = match backend_info {
            wgpu::Backend::Vulkan => BackendType::GpuVulkan,
            wgpu::Backend::Metal => BackendType::GpuMetal,
            _ => return Ok(None),
        };

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Curd Batch Hasher"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SHA256 Shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("sha256.wgsl"))),
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("SHA256 Pipeline"),
            layout: None,
            module: &shader,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });

        Ok(Some(Self::Embedded {
            device,
            queue,
            compute_pipeline,
            backend_type,
        }))
    }

    #[cfg(feature = "gpu-embedded")]
    pub async fn hash_batch(&self, items: &[&str]) -> Result<Vec<String>> {
        match self {
            Self::Embedded {
                device,
                queue,
                compute_pipeline,
                ..
            } => {
                self.hash_batch_embedded(device, queue, compute_pipeline, items)
                    .await
            }
            Self::External { worker, .. } => self.hash_batch_external(worker, items),
        }
    }

    #[cfg(not(feature = "gpu-embedded"))]
    pub async fn hash_batch(&self, items: &[&str]) -> Result<Vec<String>> {
        match self {
            Self::External { worker, .. } => self.hash_batch_external(worker, items),
        }
    }

    fn hash_batch_external(
        &self,
        worker: &std::sync::Mutex<std::process::Child>,
        items: &[&str],
    ) -> Result<Vec<String>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }

        let mut child = worker.lock().unwrap_or_else(|e| e.into_inner());

        #[derive(serde::Serialize)]
        struct Request<'a> {
            strings: &'a [&'a str],
        }

        #[derive(serde::Deserialize)]
        struct Response {
            hashes: Vec<String>,
        }

        let req = Request { strings: items };
        let req_json = serde_json::to_string(&req)?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            writeln!(stdin, "{}", req_json)?;
            child.stdin = Some(stdin);
        } else {
            return Err(anyhow::anyhow!("Worker stdin unavailable"));
        }

        if let Some(stdout) = child.stdout.take() {
            use std::io::BufRead;
            let mut reader = std::io::BufReader::new(stdout);
            let mut line = String::new();
            reader.read_line(&mut line)?;
            child.stdout = Some(reader.into_inner());

            if line.is_empty() {
                return Err(anyhow::anyhow!("Worker closed stdout unexpectedly"));
            }
            let res: Response = serde_json::from_str(&line)?;
            Ok(res.hashes)
        } else {
            Err(anyhow::anyhow!("Worker stdout unavailable"))
        }
    }

    #[cfg(feature = "gpu-embedded")]
    async fn hash_batch_embedded(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        compute_pipeline: &wgpu::ComputePipeline,
        items: &[&str],
    ) -> Result<Vec<String>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }

        let num_items = items.len() as u32;

        // 1. Prepare message padding and layout
        let mut total_blocks = 0u32;
        for s in items {
            let _bit_len = (s.len() * 8) as u64;
            let padding_len = 64 - ((s.len() + 8) % 64);
            let padded_len = s.len() + (if padding_len == 0 { 64 } else { padding_len }) + 8;
            total_blocks += (padded_len / 4) as u32; // bytes to u32s
        }

        let mut input_data: Vec<u32> = Vec::with_capacity((num_items * 2 + total_blocks) as usize);

        let mut offsets = Vec::with_capacity(num_items as usize);
        let mut block_counts = Vec::with_capacity(num_items as usize);
        let mut current_offset = 0u32;

        let mut packed_messages = Vec::with_capacity(total_blocks as usize);

        for s in items {
            let bytes = s.as_bytes();
            let bit_len = (bytes.len() * 8) as u64;
            let mut padded = bytes.to_vec();
            padded.push(0x80);

            while (padded.len() % 64) != 56 {
                padded.push(0);
            }
            padded.extend_from_slice(&bit_len.to_be_bytes());

            let u32s: Vec<u32> = padded
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();

            offsets.push(current_offset);
            block_counts.push(u32s.len() as u32);
            current_offset += u32s.len() as u32;

            packed_messages.extend(u32s);
        }

        input_data.extend(offsets);
        input_data.extend(block_counts);
        input_data.extend(packed_messages);

        // 2. Buffers
        let inputs_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Inputs Buffer"),
            size: (input_data.len() * 4) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let outputs_size = (num_items * 8 * 4) as wgpu::BufferAddress; // 8 u32s per hash
        let outputs_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Outputs Buffer"),
            size: outputs_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let num_items_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Num Items Buffer"),
            size: 4,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        queue.write_buffer(&inputs_buf, 0, bytemuck::cast_slice(&input_data));
        queue.write_buffer(&num_items_buf, 0, bytemuck::cast_slice(&[num_items]));

        let bind_group_layout = compute_pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Compute Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: inputs_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: outputs_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: num_items_buf.as_entire_binding(),
                },
            ],
        });

        // 3. Encode
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            cpass.set_pipeline(compute_pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            let workgroups = num_items.div_ceil(64);
            cpass.dispatch_workgroups(workgroups, 1, 1);
        }

        let staging_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: outputs_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        encoder.copy_buffer_to_buffer(&outputs_buf, 0, &staging_buf, 0, outputs_size);
        queue.submit(Some(encoder.finish()));

        // 4. Map & Read
        let buf_slice = staging_buf.slice(..);
        let (sender, receiver) = futures_intrusive::channel::shared::oneshot_channel();
        buf_slice.map_async(wgpu::MapMode::Read, move |v| sender.send(v).unwrap());

        device.poll(wgpu::Maintain::Wait); // Block until mapped
        receiver.receive().await.unwrap()?;

        let data = buf_slice.get_mapped_range();
        let result_u32s: &[u32] = bytemuck::cast_slice(&data);

        let mut hashes = Vec::with_capacity(num_items as usize);
        for i in 0..num_items as usize {
            let mut hash_hex = String::with_capacity(64);
            for j in 0..8 {
                let bytes = result_u32s[i * 8 + j].to_be_bytes();
                for b in bytes {
                    hash_hex.push_str(&format!("{:02x}", b));
                }
            }
            hashes.push(hash_hex);
        }

        drop(data);
        staging_buf.unmap();

        Ok(hashes)
    }

    /// Pure CPU fallback for environments where gpu-embedded is inactive
    /// or GPU is unavailable.
    pub fn hash_batch_cpu(items: &[&str]) -> Vec<String> {
        let mut hashes = Vec::with_capacity(items.len());
        for item in items {
            let mut hasher = sha2::Sha256::new();
            sha2::Digest::update(&mut hasher, item.as_bytes());
            let hash_bytes = sha2::Digest::finalize(hasher);
            let mut hash_hex = String::with_capacity(64);
            for b in hash_bytes {
                hash_hex.push_str(&format!("{:02x}", b));
            }
            hashes.push(hash_hex);
        }
        hashes
    }
}
