use cudarc::driver::{CudaDevice, CudaFunction, CudaSlice, LaunchConfig, LaunchAsync};
use cudarc::nvrtc::Ptx;
use cudarc::driver::sys::CUdevice_attribute_enum;

use std::io::{Error, ErrorKind};
use std::sync::Arc;
use log::{error, warn, info};
use crate::models::graph::GraphData;
use std::collections::HashMap;
use crate::models::simulation_params::SimulationParams;
use crate::utils::socket_flow_messages::BinaryNodeData;
use crate::types::vec3::Vec3Data;
use std::path::Path;
use std::env;
use tokio::sync::RwLock;
use std::time::Duration;
use tokio::time::sleep;

// Constants for GPU computation
const BLOCK_SIZE: u32 = 256;
const MAX_NODES: u32 = 1_000_000;
const NODE_SIZE: u32 = std::mem::size_of::<BinaryNodeData>() as u32;
const SHARED_MEM_SIZE: u32 = BLOCK_SIZE * NODE_SIZE;
// Constants for retry mechanism
const MAX_GPU_INIT_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 500; // 500ms delay between retries

// Note: CPU fallback code has been removed as we're always using GPU now

#[derive(Debug)]
pub struct GPUCompute {
    pub device: Arc<CudaDevice>,
    pub force_kernel: CudaFunction,
    pub node_data: CudaSlice<BinaryNodeData>,
    pub num_nodes: u32,
    pub node_indices: HashMap<String, usize>,
    pub simulation_params: SimulationParams,
    pub iteration_count: i32,
}

impl GPUCompute {
    pub fn test_gpu() -> Result<(), Error> {
        info!("Running GPU test");
        
        // Try to create a device using our helper function
        let device = Self::create_cuda_device()?;
        
        // Try to allocate and manipulate some memory
        let test_data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let gpu_data = device.alloc_zeros::<f32>(5)
            .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;
        
        device.dtoh_sync_copy_into(&gpu_data, &mut test_data.clone())
            .map_err(|e| Error::new(ErrorKind::Other, e.to_string()))?;
        
        info!("GPU test successful");
        Ok(())
    }
    
    fn create_cuda_device() -> Result<Arc<CudaDevice>, Error> {
        // First try to use the NVIDIA_GPU_UUID environment variable
        if let Ok(uuid) = env::var("NVIDIA_GPU_UUID") {
            info!("Attempting to create CUDA device with UUID: {}", uuid);
            // Note: cudarc doesn't directly support creation by UUID, so we log it
            // but setting NVIDIA_VISIBLE_DEVICES in the container handles this instead
            info!("Using GPU UUID {} via environment variables", uuid);
            
            // Check if CUDA_VISIBLE_DEVICES is set, which may override device index
            if let Ok(devices) = env::var("CUDA_VISIBLE_DEVICES") {
                info!("CUDA_VISIBLE_DEVICES is set to: {}", devices);
            }
        }
        
        // Always use device index 0 within the container
        // (NVIDIA_VISIBLE_DEVICES in docker-compose.yml controls which actual GPU this is)
        info!("Creating CUDA device with index 0");
        match CudaDevice::new(0) {
            Ok(device) => {
                // Successfully created device
                info!("Successfully created CUDA device with index 0 (for GPU UUID: {})", env::var("NVIDIA_GPU_UUID").unwrap_or_else(|_| "unknown".to_string()));
                Ok(device.into()) // Use .into() to convert to Arc
            },
            Err(e) => {
                error!("Failed to create CUDA device with index 0: {}", e);
                Err(Error::new(ErrorKind::Other, 
                    format!("Failed to create CUDA device: {}. Make sure CUDA drivers are installed and working, and GPU is properly detected.", e)))
            }
        }
    }

    pub async fn new(graph: &GraphData) -> Result<Arc<RwLock<Self>>, Error> {
        let num_nodes = graph.nodes.len() as u32;
        info!("Initializing GPU compute with {} nodes (with retry mechanism)", num_nodes);

        // Check node count limit
        if num_nodes > MAX_NODES {
            return Err(Error::new(
                std::io::ErrorKind::Other,
                format!("Node count exceeds limit: {}", MAX_NODES),
            ));
        }

        // Use retry mechanism for GPU initialization
        Self::with_retry(MAX_GPU_INIT_RETRIES, RETRY_DELAY_MS, |attempt| async move {
            Self::initialize_gpu(graph, num_nodes, attempt).await
        }).await
    }
    
    fn test_gpu_capabilities() -> Result<(), Error> {
        // Check if CUDA is available
        info!("Testing CUDA capabilities");
        
        // Log environment variables for diagnostic purposes
        match env::var("NVIDIA_GPU_UUID") {
            Ok(uuid) => info!("NVIDIA_GPU_UUID is set to: {}", uuid),
            Err(_) => warn!("NVIDIA_GPU_UUID environment variable is not set")
        }
        
        // Try to get CUDA device count
        match CudaDevice::count() {
            Ok(count) => {
                if count == 0 {
                    return Err(Error::new(ErrorKind::NotFound, 
                        "No CUDA devices found. Check if NVIDIA drivers are installed and working."));
                }
                info!("Found {} CUDA device(s)", count);
                Ok(())
            },
            Err(e) => {
                error!("Failed to get CUDA device count: {}", e);
                Err(Error::new(ErrorKind::Other, 
                    format!("Failed to get CUDA device count: {}. Check if NVIDIA drivers are installed and working.", e)))
            }
        }
    }
    
    fn diagnostic_cuda_info() -> Result<(), Error> {
        info!("Running CUDA diagnostic checks");
        
        // Environment variables
        info!("Checking CUDA-related environment variables:");
        for var in &["NVIDIA_GPU_UUID", "NVIDIA_VISIBLE_DEVICES", "CUDA_VISIBLE_DEVICES"] {
            match env::var(var) {
                Ok(val) => info!("  {}={}", var, val),
                Err(_) => info!("  {} is not set", var)
            }
        }
        
        // Try to get device count
        match CudaDevice::count() {
            Ok(count) => info!("CUDA device count: {}", count),
            Err(e) => error!("Failed to get CUDA device count: {}", e)
        }
        
        Ok(())
    }
    
    async fn initialize_gpu(graph: &GraphData, num_nodes: u32, attempt: u32) -> Result<Arc<RwLock<Self>>, Error> {
        info!("GPU initialization attempt {}/{}", attempt + 1, MAX_GPU_INIT_RETRIES);
        
        // Check device capabilities
        match Self::test_gpu_capabilities() {
            Ok(_) => info!("GPU capabilities check passed"),
            Err(e) => {
                warn!("GPU capabilities check failed on attempt {}: {}", attempt + 1, e);
                return Err(e);
            }
        }

        info!("Attempting to create CUDA device (attempt {}/{})", attempt + 1, MAX_GPU_INIT_RETRIES);
        let device = match Self::create_cuda_device() {
            Ok(dev) => {
                info!("CUDA device created successfully");
                let max_threads = match dev.as_ref().attribute(CUdevice_attribute_enum::CU_DEVICE_ATTRIBUTE_MAX_THREADS_PER_BLOCK as _) {
                    Ok(val) => val,
                    Err(e) => {
                        warn!("Failed to get max threads attribute: {} (attempt {}/{})", e, attempt + 1, MAX_GPU_INIT_RETRIES);
                        return Err(Error::new(ErrorKind::Other, e.to_string()));
                    }
                };
                
                let compute_mode = match dev.as_ref().attribute(CUdevice_attribute_enum::CU_DEVICE_ATTRIBUTE_COMPUTE_MODE as _) {
                    Ok(val) => val,
                    Err(e) => {
                        warn!("Failed to get compute mode attribute: {} (attempt {}/{})", e, attempt + 1, MAX_GPU_INIT_RETRIES);
                        return Err(Error::new(ErrorKind::Other, e.to_string()));
                    }
                };
                
                let multiprocessor_count = match dev.as_ref().attribute(CUdevice_attribute_enum::CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT as _) {
                    Ok(val) => val,
                    Err(e) => {
                        warn!("Failed to get multiprocessor count attribute: {} (attempt {}/{})", e, attempt + 1, MAX_GPU_INIT_RETRIES);
                        return Err(Error::new(ErrorKind::Other, e.to_string()));
                    }
                };
                
                info!("GPU Device detected:");
                info!("  Max threads per MP: {}", max_threads);
                info!("  Multiprocessor count: {}", multiprocessor_count);
                info!("  Compute mode: {}", compute_mode);
                
                if max_threads < 256 {
                    let err = Error::new(ErrorKind::Other, 
                        format!("GPU capability too low. Device supports only {} threads per multiprocessor, minimum required is 256", 
                            max_threads));
                    warn!("GPU capability check failed: {}", err);
                    return Err(err);
                }
                dev
            },
            Err(e) => {
                error!("Failed to create CUDA device (attempt {}/{}): {}", attempt + 1, MAX_GPU_INIT_RETRIES, e);
                Self::diagnostic_cuda_info()?;
                return Err(Error::new(ErrorKind::Other, 
                   format!("Failed to create CUDA device: {}. See logs for diagnostic information.", e)));
            }
        };

        info!("Proceeding to load compute kernel (attempt {}/{})", attempt + 1, MAX_GPU_INIT_RETRIES);
        Self::load_compute_kernel(device, num_nodes, graph).await
    }
    
    /// Helper function to retry an operation with exponential backoff
    async fn with_retry<F, Fut, T>(max_attempts: u32, base_delay_ms: u64, operation: F) -> Result<T, Error>
    where
        F: Fn(u32) -> Fut,
        Fut: std::future::Future<Output = Result<T, Error>>,
    {
        let mut last_error: Option<Error> = None;
        
        for attempt in 0..max_attempts {
            match operation(attempt).await {
                Ok(result) => {
                    if attempt > 0 {
                        info!("Operation succeeded after {} retries", attempt);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    let delay = base_delay_ms * (1 << attempt); // Exponential backoff
                    warn!("Operation failed (attempt {}/{}): {}. Retrying in {}ms...", 
                          attempt + 1, max_attempts, e, delay);
                    last_error = Some(e);
                    
                    if attempt + 1 < max_attempts {
                        sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }
        
        // If we get here, all attempts failed
        error!("Operation failed after {} attempts", max_attempts);
        Err(last_error.unwrap_or_else(|| Error::new(ErrorKind::Other, 
            format!("All {} retry attempts failed", max_attempts))))
    }
    
    async fn load_compute_kernel(
        device: Arc<CudaDevice>, 
        num_nodes: u32, 
        graph: &GraphData
    ) -> Result<Arc<RwLock<Self>>, Error> {
        let ptx_path = "/app/src/utils/compute_forces.ptx";
        
        // Validate PTX file
        let ptx_path_obj = Path::new(ptx_path);

        if !ptx_path_obj.exists() {
            error!("PTX file does not exist at {} - this file is required for GPU physics", ptx_path);
            return Err(Error::new(ErrorKind::NotFound, 
                format!("PTX file not found at {}", ptx_path)));
        }

        let ptx = Ptx::from_file(ptx_path);

        info!("Successfully loaded PTX file");
            
        device.load_ptx(ptx, "compute_forces_kernel", &["compute_forces_kernel"])
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            
        let force_kernel = device.get_func("compute_forces_kernel", "compute_forces_kernel")
            .ok_or_else(|| Error::new(std::io::ErrorKind::Other, "Function compute_forces_kernel not found"))?;

        info!("Allocating device memory for {} nodes", num_nodes);
        let node_data = device.alloc_zeros::<BinaryNodeData>(num_nodes as usize)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        info!("Creating GPU compute instance");
        // Create node ID to index mapping
        let mut node_indices = HashMap::new();
        for (idx, node) in graph.nodes.iter().enumerate() {
            node_indices.insert(node.id.clone(), idx);
        }

        let mut instance = Self {
            device: Arc::clone(&device),
            force_kernel,
            node_data,
            num_nodes,
            node_indices,
            simulation_params: SimulationParams::default(),
            iteration_count: 0,
        };

        info!("Copying initial graph data to device memory");
        instance.update_graph_data(graph)?;

        info!("GPU compute initialization complete");
        Ok(Arc::new(RwLock::new(instance)))
    }

    pub fn update_graph_data(&mut self, graph: &GraphData) -> Result<(), Error> {
        info!("Updating graph data for {} nodes", graph.nodes.len());

        // First, update the node index mapping
        self.node_indices.clear();
        for (idx, node) in graph.nodes.iter().enumerate() {
            self.node_indices.insert(node.id.clone(), idx);
        }

        // Reallocate buffer if the node count has changed
        if graph.nodes.len() as u32 != self.num_nodes {
            info!("Reallocating GPU buffer for {} nodes", graph.nodes.len());
            self.node_data = self.device.alloc_zeros::<BinaryNodeData>(graph.nodes.len())
                .map_err(|e| Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            self.num_nodes = graph.nodes.len() as u32;
            
            // Reset iteration counter since we're essentially starting a new simulation
            self.iteration_count = 0;
        }

        // Prepare node data for GPU transfer
        let mut node_data = Vec::with_capacity(graph.nodes.len());

        // Debug the first few nodes to help with troubleshooting
        if graph.nodes.len() > 0 {
            let sample_size = std::cmp::min(3, graph.nodes.len());
            info!("Sample of first {} nodes before GPU transfer:", sample_size);
            for i in 0..sample_size {
                let node = &graph.nodes[i];
                info!(
                    "Node[{}] id={}: pos=[{:.3},{:.3},{:.3}], vel=[{:.3},{:.3},{:.3}]",
                    i, node.id,
                    node.data.position.x, node.data.position.y, node.data.position.z,
                    node.data.velocity.x, node.data.velocity.y, node.data.velocity.z
                );
            }
        }

        // Process each node and prepare it for GPU processing
        for node in &graph.nodes {
            // Now the memory layouts match directly between Rust and CUDA
            node_data.push(BinaryNodeData {
                position: node.data.position.clone(),
                velocity: node.data.velocity.clone(),
                mass: node.data.mass,
                flags: node.data.flags, 
                padding: node.data.padding,
            });
            
            // Log a sample of nodes to verify correct data transfer
            if node.id == "0" || node.id == "1" {
                info!(
                    "Node {} data prepared for GPU: position=[{:.3},{:.3},{:.3}], velocity=[{:.3},{:.3},{:.3}]",
                    node.id,
                    node.data.position.x, node.data.position.y, node.data.position.z,
                    node.data.velocity.x, node.data.velocity.y, node.data.velocity.z
                );
            }
        }

        info!("Copying {} nodes to GPU", graph.nodes.len());

        // Copy data to GPU memory
        self.device.htod_sync_copy_into(&node_data, &mut self.node_data)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, 
                format!("Failed to copy node data to GPU: {}", e)))?;

        Ok(())
    }

    pub fn update_simulation_params(&mut self, params: &SimulationParams) -> Result<(), Error> {
        info!("Updating simulation parameters: {:?}", params);
        self.simulation_params = params.clone();
        Ok(())
    }

    pub fn compute_forces(&mut self) -> Result<(), Error> {
        info!("Starting force computation on GPU");
        
        let blocks = ((self.num_nodes + BLOCK_SIZE - 1) / BLOCK_SIZE).max(1);
        let cfg = LaunchConfig {
            grid_dim: (blocks, 1, 1),
            block_dim: (BLOCK_SIZE, 1, 1),
            shared_mem_bytes: SHARED_MEM_SIZE,
        };

        info!("Launch config: blocks={}, threads={}, shared_mem={}",
            blocks, BLOCK_SIZE, SHARED_MEM_SIZE);

        unsafe {
            self.force_kernel.clone().launch(cfg, (
                &self.node_data,
                self.num_nodes as i32,
                self.simulation_params.spring_strength,
                self.simulation_params.damping,
                self.simulation_params.repulsion,
                self.simulation_params.time_step,
                self.simulation_params.max_repulsion_distance,
                if self.simulation_params.enable_bounds {
                    self.simulation_params.viewport_bounds
                } else {
                    f32::MAX // Effectively disable bounds
                },
                self.iteration_count,
            )).map_err(|e| {
                error!("Kernel launch failed: {}", e);
                Error::new(ErrorKind::Other, e.to_string())
            })?;
        }

        info!("Force computation completed");
        self.iteration_count += 1;
        Ok(())
    }

    pub fn get_node_data(&self) -> Result<Vec<BinaryNodeData>, Error> {
        // Create a buffer for GPU to copy data into
        let mut gpu_raw_data = vec![BinaryNodeData {
            position: Vec3Data::zero(),
            velocity: Vec3Data::zero(),
            mass: 0,
            flags: 0,
            padding: [0, 0],
        }; self.num_nodes as usize];

        // Copy data from GPU to our buffer
        self.device.dtoh_sync_copy_into(&self.node_data, &mut gpu_raw_data)
            .map_err(|e| Error::new(std::io::ErrorKind::Other, 
                format!("Failed to copy data from GPU: {}", e)))?;

        // Debug the first few nodes retrieved from GPU
        if gpu_raw_data.len() > 0 {
            let sample_size = std::cmp::min(5, gpu_raw_data.len());
            info!("Sample of first {} nodes after GPU calculation:", sample_size);
            for i in 0..sample_size {
                let node = &gpu_raw_data[i];
                
                // Calculate force magnitude for velocity verification
                let force_mag = (
                    node.velocity.x * node.velocity.x +
                    node.velocity.y * node.velocity.y +
                    node.velocity.z * node.velocity.z
                ).sqrt();
                info!(
                    "Node[{}]: force_mag={:.6}, pos=[{:.3},{:.3},{:.3}], vel=[{:.6},{:.6},{:.6}]",
                    i, force_mag,
                    node.position.x, node.position.y, node.position.z,
                    node.velocity.x, node.velocity.y, node.velocity.z
                );
            }
        }

        // Return the retrieved data
        let gpu_nodes = gpu_raw_data.into_iter().map(|raw_node| {
            BinaryNodeData {
                position: raw_node.position.clone(),
                velocity: raw_node.velocity.clone(),
                mass: raw_node.mass,
                flags: raw_node.flags,
                padding: raw_node.padding,
            }
        }).collect();
        
        Ok(gpu_nodes)
    }

    // For GPU kernels that need raw array access, we'll add helper methods 
    // to convert Vec3Data to arrays when needed

    pub fn step(&mut self) -> Result<(), Error> {
        info!("Executing physics step (iteration {})", self.iteration_count);
        self.compute_forces()?;

        if self.iteration_count % 60 == 0 {
            // Log detailed information every 60 iterations
            info!("Physics simulation status:");
            info!("  - Iteration count: {}", self.iteration_count);
            info!("  - Node count: {}", self.num_nodes);
            info!("  - Spring strength: {}", self.simulation_params.spring_strength);
            info!("  - Repulsion: {}", self.simulation_params.repulsion);
            info!("  - Damping: {}", self.simulation_params.damping);
        } else {
            // Otherwise just log a quick summary
            info!("Physics step complete, iteration count: {}", self.iteration_count);
        }
        Ok(())
    }
    
    /// Run a minimal test computation to verify that the GPU instance is working properly
    pub fn test_compute(&self) -> Result<(), Error> {
        info!("Running test computation on GPU instance");

        // Try to run a simple operation on the device
        match self.device.synchronize() {
            Ok(_) => {
                info!("GPU device access test passed");
            },
            Err(e) => {
                error!("GPU device access test failed: {}", e);
                return Err(Error::new(ErrorKind::Other, format!("GPU device access test failed: {}", e)));            }
        }
        
        // If we got here, the GPU instance is working
        info!("GPU test computation successful");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gpu_compute_initialization() {
        info!("Running GPU compute initialization test");
        let graph = GraphData::default();
        let gpu_compute = GPUCompute::new(&graph).await;
        assert!(gpu_compute.is_ok());
    }

    #[tokio::test]
    async fn test_node_data_transfer() {
        info!("Running node data transfer test");
        let mut graph = GraphData::default();
        let gpu_compute = GPUCompute::new(&graph).await.unwrap();
        let gpu_compute = Arc::try_unwrap(gpu_compute).unwrap().into_inner();
        let node_data = gpu_compute.get_node_data().unwrap();
        assert_eq!(node_data.len(), graph.nodes.len());
    }

    #[test]
    fn test_node_data_memory_layout() {
        info!("Checking BinaryNodeData memory layout");
        use std::mem::size_of;
        assert_eq!(size_of::<BinaryNodeData>(), 28); // 24 bytes for position/velocity + 4 bytes for mass/flags/padding
    }
}
