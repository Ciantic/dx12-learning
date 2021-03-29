use bindings::{
    windows::win32::direct3d11::*, windows::win32::direct3d12::*, windows::win32::direct3d_hlsl::*,
    windows::win32::direct_composition::*, windows::win32::display_devices::*,
    windows::win32::dxgi::*, windows::win32::gdi::*, windows::win32::keyboard_and_mouse_input::*,
    windows::win32::menus_and_resources::*, windows::win32::system_services::*,
    windows::win32::windows_and_messaging::*,
};
use directx_math::*;
use dx12_common::{
    cd3dx12_blend_desc_default, cd3dx12_depth_stencil_desc_default,
    cd3dx12_heap_properties_with_type, cd3dx12_rasterizer_desc_default,
    cd3dx12_resource_barrier_transition, create_default_buffer, UploadBuffer,
};
use std::{borrow::BorrowMut, ptr::null_mut};
use std::{convert::TryInto, ffi::CString};
use windows::{Abi, Interface};

const NUM_OF_FRAMES: usize = 3;

#[derive(Debug)]
#[repr(C)]
struct SceneConstantBuffer {
    /// Projection transformation matrix
    proj: XMFLOAT4X4,

    /// View transformation matrix
    view: XMFLOAT4X4,
}

#[derive(Debug)]
#[repr(C)]
struct ObjectConstantBuffer {
    /// World transformation matrix
    ///
    /// This determines the location/orientation/scale of the single cube in the
    /// world
    world: XMFLOAT4X4,
}

#[derive(Debug)]
#[repr(C)]
struct Vertex {
    position: XMFLOAT3,
    color: XMFLOAT4,
}
impl Vertex {
    fn new(position: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            position: position.into(),
            color: color.into(),
        }
    }
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const MAGENTA: [f32; 4] = [1.0, 0.0, 1.0, 1.0];
const YELLOW: [f32; 4] = [1.0, 1.0, 0.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

#[derive(Debug)]
#[repr(C)]
struct FrameResource {
    fence_value: u64,
    allocator: ID3D12CommandAllocator,
    list: ID3D12GraphicsCommandList,
    scene_cb: UploadBuffer<SceneConstantBuffer>,
    object_cb: UploadBuffer<ObjectConstantBuffer>,
}

impl FrameResource {
    pub fn new(device: &ID3D12Device, pso: &ID3D12PipelineState) -> Self {
        // Create allocator for the frame
        let allocator = unsafe {
            let mut ptr: Option<ID3D12CommandAllocator> = None;
            device
                .CreateCommandAllocator(
                    D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &ID3D12CommandAllocator::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
        }
        .expect("Unable to create allocator");

        // Create command list for the frame
        let list = unsafe {
            let mut ptr: Option<ID3D12GraphicsCommandList> = None;
            device
                .CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &allocator,
                    pso,
                    &ID3D12GraphicsCommandList::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
        }
        .expect("Unable to create command list");

        // Command list must be closed on create
        unsafe {
            list.Close().ok().expect("Unable to close the list");
        }

        let scene_cb = UploadBuffer::new(
            // &cbv_heap,
            &device,
            &SceneConstantBuffer {
                ..unsafe { std::mem::zeroed() }
            },
        )
        .unwrap();

        let object_cb = UploadBuffer::new(
            &device,
            &ObjectConstantBuffer {
                world: {
                    let world = XMMatrixIdentity();
                    let world = XMMatrixMultiply(world, &XMMatrixScaling(0.3, 0.3, 0.3));
                    let world = XMMatrixMultiply(world, &XMMatrixTranslation(0.0, 0.0, 0.0));
                    let mut out: XMFLOAT4X4 = unsafe { std::mem::zeroed() };
                    XMStoreFloat4x4(&mut out, world);
                    out
                },
            },
        )
        .expect("Got it");

        FrameResource {
            fence_value: 0,
            allocator,
            list,
            scene_cb,
            object_cb,
        }
    }

    pub fn update_constant_buffers(&mut self, camera: &Camera) {
        let (proj, view) = camera.get_proj_view(XM_PIDIV2, 1.0, 128.0, 1024.0, 1024.0);
        self.scene_cb.update(&SceneConstantBuffer { view, proj })
    }
}

struct Camera {
    eye: XMVECTOR,
    at: XMVECTOR,
    up: XMVECTOR,
}

/// Camera
///
/// This closely follows:
/// https://github.com/microsoft/DirectX-Graphics-Samples/blob/master/Samples/Desktop/D3D12Multithreading/src/Camera.cpp
impl Camera {
    pub fn get_proj_view(
        &self,
        fov_deg: f32,
        near_z: f32,
        far_z: f32,
        width: f32,
        height: f32,
    ) -> (XMFLOAT4X4, XMFLOAT4X4) {
        let ar = width / height;
        let fov_angle_y = if ar < 1.0 {
            fov_deg * XM_PI / 180.0 / ar
        } else {
            fov_deg * XM_PI / 180.0
        };
        let mut view: XMFLOAT4X4 = unsafe { std::mem::zeroed() };
        let mut proj: XMFLOAT4X4 = unsafe { std::mem::zeroed() };

        XMStoreFloat4x4(
            &mut view,
            XMMatrixTranspose(XMMatrixLookAtLH(self.eye, self.at, self.up)),
        );
        XMStoreFloat4x4(
            &mut proj,
            XMMatrixTranspose(XMMatrixPerspectiveFovLH(fov_angle_y, ar, near_z, far_z)),
        );
        (proj, view)
    }

    pub fn rotate_yaw(&mut self, deg: f32) {
        let rotation = XMMatrixRotationAxis(self.up, deg);
        self.eye = XMVector3TransformCoord(self.eye, rotation);
    }

    pub fn rotate_pitch(&mut self, deg: f32) {
        let right = XMVector3Normalize(XMVector3Cross(self.eye, self.up));
        let rotation = XMMatrixRotationAxis(right, deg);
        self.eye = XMVector3TransformCoord(self.eye, rotation);
    }
}

#[allow(dead_code)]
struct Window {
    hwnd: HWND,
    factory: IDXGIFactory4,
    adapter: IDXGIAdapter1,
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    comp_device: IDCompositionDevice,
    swap_chain: IDXGISwapChain3,
    current_frame: usize,
    comp_target: IDCompositionTarget,
    comp_visual: IDCompositionVisual,
    rtv_desc_heap: ID3D12DescriptorHeap,
    rtv_desc_size: usize,
    back_buffers: [ID3D12Resource; NUM_OF_FRAMES],
    depth_stencil_heap: ID3D12DescriptorHeap,
    depth_stencil_buffer: ID3D12Resource,
    root_signature: ID3D12RootSignature,
    vertex_shader: ID3DBlob,
    pixel_shader: ID3DBlob,
    pipeline_state: ID3D12PipelineState,
    viewport: D3D12_VIEWPORT,
    scissor: RECT,

    fence: ID3D12Fence,
    fence_event: HANDLE,
    fence_value: u64,

    // Resources
    vertex_buffer: ID3D12Resource,
    vertex_buffer_view: D3D12_VERTEX_BUFFER_VIEW,

    indices_buffer: ID3D12Resource,
    indices_buffer_view: D3D12_INDEX_BUFFER_VIEW,

    frame_resources: [FrameResource; NUM_OF_FRAMES],
    camera: Camera,
}

impl Window {
    pub fn new(hwnd: HWND) -> windows::Result<Self> {
        // Start "DebugView" to listen errors
        // https://docs.microsoft.com/en-us/sysinternals/downloads/debugview
        let debug = unsafe {
            let mut ptr: Option<ID3D12Debug> = None;
            D3D12GetDebugInterface(&ID3D12Debug::IID, ptr.set_abi()).and_some(ptr)
        }
        .expect("Unable to create debug layer");

        unsafe {
            debug.EnableDebugLayer();
        }

        let factory = unsafe {
            let mut ptr: Option<IDXGIFactory4> = None;
            CreateDXGIFactory2(0, &IDXGIFactory4::IID, ptr.set_abi()).and_some(ptr)
        }?;

        let adapter = (0..99)
            .into_iter()
            .find_map(|i| unsafe {
                let mut ptr: Option<IDXGIAdapter1> = None;
                factory.EnumAdapters1(i, &mut ptr).and_some(ptr).ok()
            })
            .expect("Could not find d3d adapter");

        let device = unsafe {
            let mut ptr: Option<ID3D12Device> = None;
            D3D12CreateDevice(
                &adapter, // None for default adapter
                D3D_FEATURE_LEVEL::D3D_FEATURE_LEVEL_11_0,
                &ID3D12Device::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)
        }?;

        let queue = unsafe {
            let mut ptr: Option<ID3D12CommandQueue> = None;
            let desc = D3D12_COMMAND_QUEUE_DESC {
                r#type: D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                priority: D3D12_COMMAND_QUEUE_PRIORITY::D3D12_COMMAND_QUEUE_PRIORITY_HIGH.0,
                flags: D3D12_COMMAND_QUEUE_FLAGS::D3D12_COMMAND_QUEUE_FLAG_NONE,
                node_mask: 0,
            };
            device
                .CreateCommandQueue(&desc, &ID3D12CommandQueue::IID, ptr.set_abi())
                .and_some(ptr)
        }?;

        // let allocators: [ID3D12CommandAllocator; NUM_OF_FRAMES] = (0..NUM_OF_FRAMES)
        //     .map(|_| unsafe {
        //         let mut ptr: Option<ID3D12CommandAllocator> = None;
        //         device
        //             .CreateCommandAllocator(
        //                 D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
        //                 &ID3D12CommandAllocator::IID,
        //                 ptr.set_abi(),
        //             )
        //             .and_some(ptr)
        //             .expect("Unable to create allocator")
        //     })
        //     .collect::<Vec<_>>()
        //     .try_into()
        //     .expect("Unable to create allocators");

        // Composition device
        let comp_device = unsafe {
            let mut ptr: Option<IDCompositionDevice> = None;
            DCompositionCreateDevice(None, &IDCompositionDevice::IID, ptr.set_abi()).and_some(ptr)
        }?;

        // Create swap chain for composition
        let swap_chain = unsafe {
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                alpha_mode: DXGI_ALPHA_MODE::DXGI_ALPHA_MODE_PREMULTIPLIED,
                buffer_count: NUM_OF_FRAMES as _,
                width: 1024,
                height: 1024,
                format: DXGI_FORMAT::DXGI_FORMAT_B8G8R8A8_UNORM,
                flags: 0,
                buffer_usage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                sample_desc: DXGI_SAMPLE_DESC {
                    count: 1,
                    quality: 0,
                },
                scaling: DXGI_SCALING::DXGI_SCALING_STRETCH,
                stereo: BOOL(0),
                swap_effect: DXGI_SWAP_EFFECT::DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            };
            let mut ptr: Option<IDXGISwapChain1> = None;
            factory
                .CreateSwapChainForComposition(&queue, &desc, None, &mut ptr)
                .and_some(ptr)
        }?
        .cast::<IDXGISwapChain3>()?;

        // Current frame index
        let current_frame = unsafe { swap_chain.GetCurrentBackBufferIndex() as usize };

        // Create IDCompositionTarget for the window
        let comp_target = unsafe {
            let mut ptr = None;
            comp_device
                .CreateTargetForHwnd(hwnd, BOOL(1), &mut ptr)
                .and_some(ptr)
        }?;

        // Create IDCompositionVisual for the window
        let comp_visual = unsafe {
            let mut ptr = None;
            comp_device.CreateVisual(&mut ptr).and_some(ptr)
        }?;

        // Set swap_chain and the root visual and commit
        unsafe {
            comp_visual.SetContent(&swap_chain).ok()?;
            comp_target.SetRoot(&comp_visual).ok()?;
            comp_device.Commit().ok()?;
        }

        // Create descriptor heap for render target views
        let rtv_desc_heap = unsafe {
            let desc = D3D12_DESCRIPTOR_HEAP_DESC {
                r#type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                num_descriptors: NUM_OF_FRAMES as _,
                flags: D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                node_mask: 0,
            };
            let mut ptr: Option<ID3D12DescriptorHeap> = None;
            device
                .CreateDescriptorHeap(&desc, &ID3D12DescriptorHeap::IID, ptr.set_abi())
                .and_some(ptr)
        }?;

        // Create resource per frame
        let mut descriptor = unsafe { rtv_desc_heap.GetCPUDescriptorHandleForHeapStart() };
        let rtv_desc_size = unsafe {
            device.GetDescriptorHandleIncrementSize(
                D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            ) as usize
        };
        let back_buffers = (0..NUM_OF_FRAMES)
            .map(|i| {
                let resource = unsafe {
                    let mut ptr: Option<ID3D12Resource> = None;
                    swap_chain
                        .GetBuffer(i as _, &ID3D12Resource::IID, ptr.set_abi())
                        .and_some(ptr)
                }?;

                unsafe {
                    // let desc = D3D12_TEX2D_RTV {
                    //     Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    //     u: D3D12_RTV_DIMENSION_UNKNOWN as _,
                    //     ViewDimension: 0,
                    // };
                    device.CreateRenderTargetView(&resource, 0 as _, &descriptor);
                    descriptor.ptr += rtv_desc_size;
                }

                Ok(resource)
            })
            .collect::<Result<Vec<_>, windows::ErrorCode>>()?
            .try_into()
            .expect("Unable to create resources");

        // Create depth/stencil heap
        let depth_stencil_heap = unsafe {
            let desc = D3D12_DESCRIPTOR_HEAP_DESC {
                r#type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
                num_descriptors: 1,
                flags: D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                node_mask: 0,
            };
            let mut ptr: Option<ID3D12DescriptorHeap> = None;
            device
                .CreateDescriptorHeap(&desc, &ID3D12DescriptorHeap::IID, ptr.set_abi())
                .and_some(ptr)
        }?;

        // Create depth/stencil buffer
        let depth_stencil_buffer = unsafe {
            let mut ptr: Option<ID3D12Resource> = None;
            device
                .CreateCommittedResource(
                    &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_DEFAULT),
                    D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                    &D3D12_RESOURCE_DESC {
                        alignment: 0,
                        width: 1024,
                        height: 1024,

                        // If DXGI_SWAP_CHAIN_DESC1::Stereo is TRUE (3d glasses
                        // support) following array size needs to be 2:
                        depth_or_array_size: 1,

                        mip_levels: 1,
                        dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                        sample_desc: DXGI_SAMPLE_DESC {
                            count: 1,
                            quality: 0,
                        },
                        format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                        flags: D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
                        ..std::mem::zeroed()
                    },
                    // D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COMMON,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_DEPTH_WRITE,
                    &D3D12_CLEAR_VALUE {
                        format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                        anonymous: D3D12_CLEAR_VALUE_0 {
                            depth_stencil: D3D12_DEPTH_STENCIL_VALUE {
                                depth: 1.0,
                                stencil: 0,
                            },
                        },
                    },
                    &ID3D12Resource::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
        }?;

        unsafe {
            device.CreateDepthStencilView(
                &depth_stencil_buffer,
                null_mut(),
                // &D3D12_DEPTH_STENCIL_VIEW_DESC {
                //     format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                //     view_dimension: D3D12_DSV_DIMENSION::D3D12_DSV_DIMENSION_TEXTURE2D,
                //     flags: D3D12_DSV_FLAGS::D3D12_DSV_FLAG_NONE,

                //     ..std::mem::zeroed()
                // },
                depth_stencil_heap.GetCPUDescriptorHandleForHeapStart(),
            )
        }

        // Creation of constant buffer begins here -----------------------------
        //
        // Steps are roughly:
        //
        // 1. Create a heap
        // 2. Create a constant buffer resource as upload buffer, send your
        //    initial value there
        // 3. Assign your constant buffers to the root_signature
        //
        // Note that there needs to be as many buffers as there are frames so
        // that you don't end up updating in-use buffer. In this example however
        // the value is not updated after the initial value.

        // Create constant buffer heaps
        // let cbv_heap: ID3D12DescriptorHeap = unsafe {
        //     let mut ptr: Option<ID3D12DescriptorHeap> = None;
        //     device
        //         .CreateDescriptorHeap(
        //             &D3D12_DESCRIPTOR_HEAP_DESC {
        //                 r#type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
        //                 num_descriptors: 1,
        //                 flags:
        //                     D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        //                 node_mask: 0,
        //             },
        //             &ID3D12DescriptorHeap::IID,
        //             ptr.set_abi(),
        //         )
        //         .and_some(ptr)
        //         .unwrap()
        // };

        // Create root signature
        let root_signature = unsafe {
            let root = {
                let mut blob: Option<ID3DBlob> = None;
                let mut error: Option<ID3DBlob> = None;

                let mut params = [
                    D3D12_ROOT_PARAMETER {
                        parameter_type: D3D12_ROOT_PARAMETER_TYPE::D3D12_ROOT_PARAMETER_TYPE_CBV,
                        anonymous: D3D12_ROOT_PARAMETER_0 {
                            descriptor: D3D12_ROOT_DESCRIPTOR {
                                register_space: 0,
                                shader_register: 0,
                            },
                        },
                        shader_visibility: D3D12_SHADER_VISIBILITY::D3D12_SHADER_VISIBILITY_VERTEX,
                    },
                    D3D12_ROOT_PARAMETER {
                        parameter_type: D3D12_ROOT_PARAMETER_TYPE::D3D12_ROOT_PARAMETER_TYPE_CBV,
                        anonymous: D3D12_ROOT_PARAMETER_0 {
                            descriptor: D3D12_ROOT_DESCRIPTOR {
                                register_space: 0,
                                shader_register: 1,
                            },
                        },
                        shader_visibility: D3D12_SHADER_VISIBILITY::D3D12_SHADER_VISIBILITY_VERTEX,
                    },
                ];

                let desc = D3D12_ROOT_SIGNATURE_DESC {
                    num_parameters: params.len() as _,
                    p_parameters: params.as_mut_ptr(),
                    num_static_samplers: 0,
                    p_static_samplers: null_mut() as _,
                    flags: D3D12_ROOT_SIGNATURE_FLAGS::from(
                            D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT.0 |
                            D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS.0 |
                            D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS.0 |
                            D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS.0
                        )
                    ,
                };
                D3D12SerializeRootSignature(
                    &desc,
                    D3D_ROOT_SIGNATURE_VERSION::D3D_ROOT_SIGNATURE_VERSION_1_0,
                    &mut blob as _,
                    &mut error as _,
                )
                .and_then(|| {
                    if error.is_none() {
                        blob.unwrap()
                    } else {
                        panic!("Root signature failed, error blob contains the error")
                    }
                })
            }?;

            let mut ptr: Option<ID3D12RootSignature> = None;
            device
                .CreateRootSignature(
                    0,
                    root.GetBufferPointer(),
                    root.GetBufferSize(),
                    &ID3D12RootSignature::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
        }?;

        // End of constant buffer changes ----------------------------------

        let vertex_shader = unsafe {
            let data = include_bytes!("./05-camera.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("shaders.hlsl\0".as_ptr() as _),
                null_mut(),
                None,
                PSTR("VSMain\0".as_ptr() as _),
                PSTR("vs_5_0\0".as_ptr() as _),
                0,
                0,
                &mut ptr,
                &mut err,
            )
            .ok()?;

            match ptr {
                Some(v) => v,
                None => {
                    panic!(
                        "Shader creation failed with error {}",
                        CString::from_raw(err.unwrap().GetBufferPointer() as _).to_string_lossy()
                    )
                }
            }
        };

        let pixel_shader = unsafe {
            let data = include_bytes!("./05-camera.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("shaders.hlsl\0".as_ptr() as _),
                null_mut(),
                None,
                PSTR("PSMain\0".as_ptr() as _),
                PSTR("ps_5_0\0".as_ptr() as _),
                0,
                0,
                &mut ptr,
                &mut err,
            )
            .ok()?;

            match ptr {
                Some(v) => v,
                None => {
                    panic!(
                        "Shader creation failed with error {}",
                        CString::from_raw(err.unwrap().GetBufferPointer() as _).to_string_lossy()
                    )
                }
            }
        };

        let mut els = [
            D3D12_INPUT_ELEMENT_DESC {
                semantic_name: PSTR("POSITION\0".as_ptr() as _),
                semantic_index: 0,
                format: DXGI_FORMAT::DXGI_FORMAT_R32G32B32_FLOAT,
                input_slot: 0,
                instance_data_step_rate: 0,
                input_slot_class:
                    D3D12_INPUT_CLASSIFICATION::D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                aligned_byte_offset: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                semantic_name: PSTR("COLOR\0".as_ptr() as _),
                semantic_index: 0,
                format: DXGI_FORMAT::DXGI_FORMAT_R32G32B32A32_FLOAT,
                input_slot: 0,
                instance_data_step_rate: 0,
                input_slot_class:
                    D3D12_INPUT_CLASSIFICATION::D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                aligned_byte_offset: 12,
            },
        ];

        let pso_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            // TODO: Can I get rid of this clone? Or do I even have to?
            p_root_signature: Some(root_signature.clone()),
            // unsafe { std::mem::transmute(root_signature.abi()) },
            input_layout: D3D12_INPUT_LAYOUT_DESC {
                num_elements: els.len() as u32,
                p_input_element_descs: els.as_mut_ptr(),
            },
            vs: D3D12_SHADER_BYTECODE {
                bytecode_length: unsafe { vertex_shader.GetBufferSize() },
                p_shader_bytecode: unsafe { vertex_shader.GetBufferPointer() },
            },
            ps: D3D12_SHADER_BYTECODE {
                bytecode_length: unsafe { pixel_shader.GetBufferSize() },
                p_shader_bytecode: unsafe { pixel_shader.GetBufferPointer() },
            },
            rasterizer_state: cd3dx12_rasterizer_desc_default(),
            blend_state: cd3dx12_blend_desc_default(),
            sample_mask: 0xffffffff,
            primitive_topology_type:
                D3D12_PRIMITIVE_TOPOLOGY_TYPE::D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            num_render_targets: 1,
            rtv_formats: (0..D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT)
                .map(|i| {
                    if i == 0 {
                        DXGI_FORMAT::DXGI_FORMAT_B8G8R8A8_UNORM
                    } else {
                        DXGI_FORMAT::DXGI_FORMAT_UNKNOWN
                    }
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            sample_desc: DXGI_SAMPLE_DESC {
                count: 1,
                quality: 0,
            },
            dsv_format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
            depth_stencil_state: cd3dx12_depth_stencil_desc_default(),
            ..D3D12_GRAPHICS_PIPELINE_STATE_DESC::default()
        };

        let pipeline_state = unsafe {
            let mut ptr: Option<ID3D12PipelineState> = None;
            device
                .CreateGraphicsPipelineState(&pso_desc, &ID3D12PipelineState::IID, ptr.set_abi())
                .and_some(ptr)
        }
        .expect("Unable to create pipeline state");

        let allocator = unsafe {
            let mut ptr: Option<ID3D12CommandAllocator> = None;
            device
                .CreateCommandAllocator(
                    D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &ID3D12CommandAllocator::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
                .expect("Unable to create allocator")
        };

        // Create direct command list
        let list = unsafe {
            let mut ptr: Option<ID3D12GraphicsCommandList> = None;
            device
                .CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &allocator,
                    &pipeline_state,
                    &ID3D12GraphicsCommandList::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
        }?;
        unsafe {
            list.Close().ok()?;
        }

        let viewport = D3D12_VIEWPORT {
            width: 1024.0,
            height: 1024.0,
            max_depth: D3D12_MAX_DEPTH,
            min_depth: D3D12_MIN_DEPTH,
            top_leftx: 0.0,
            top_lefty: 0.0,
        };

        let scissor = RECT {
            top: 0,
            left: 0,
            bottom: 1024,
            right: 1024,
        };

        let camera = Camera {
            eye: XMVectorSet(15.0, 15.0, -30.0, 0.0),
            at: XMVectorSet(0.0, 0.0, 0.0, 0.0),
            up: XMVectorSet(0.0, 1.0, 0.0, 0.0),
        };

        // Resource initialization ------------------------------------------

        // Create fence
        let (fence, fence_value, fence_event) = unsafe {
            let mut ptr: Option<ID3D12Fence> = None;
            let fence = device
                .CreateFence(
                    0,
                    D3D12_FENCE_FLAGS::D3D12_FENCE_FLAG_NONE,
                    &ID3D12Fence::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)?;
            let fence_event = CreateEventA(null_mut(), false, false, PSTR(null_mut()));
            if fence_event.0 == 0 {
                panic!("Unable to create fence event");
            }
            (fence, 0, fence_event)
        };

        // Create constant buffer resources
        let frame_resources: [FrameResource; NUM_OF_FRAMES] = (0..NUM_OF_FRAMES)
            .map(|_| FrameResource::new(&device, &pipeline_state))
            .collect::<Vec<_>>()
            .try_into()
            .expect("Unable to create frame resources");

        unsafe {
            // allocators[current_frame].Reset().ok()?;
            list.Reset(&allocator, &pipeline_state).ok()?;
        }

        let (vertex_buffer, vertex_buffer_view, _vertex_buffer_upload) = unsafe {
            // -1.0, +1.0           +1.0, +1.0
            //               │
            //               │
            //               │
            //               │
            //             0,│0
            //     ──────────┼──────────
            //               │
            //               │
            //               │
            //               │
            //               │
            // -1.0, -1.0           +1.0, -1.0

            let vertices: [Vertex; 24] = [
                // front
                Vertex::new([-0.5, 0.5, -0.5], RED),
                Vertex::new([0.5, -0.5, -0.5], RED),
                Vertex::new([-0.5, -0.5, -0.5], RED),
                Vertex::new([0.5, 0.5, -0.5], RED),
                // Right
                Vertex::new([0.5, -0.5, -0.5], GREEN),
                Vertex::new([0.5, 0.5, 0.5], GREEN),
                Vertex::new([0.5, -0.5, 0.5], GREEN),
                Vertex::new([0.5, 0.5, -0.5], GREEN),
                // Left
                Vertex::new([-0.5, 0.5, 0.5], BLUE),
                Vertex::new([-0.5, -0.5, -0.5], BLUE),
                Vertex::new([-0.5, -0.5, 0.5], BLUE),
                Vertex::new([-0.5, 0.5, -0.5], BLUE),
                // Back
                Vertex::new([0.5, 0.5, 0.5], MAGENTA),
                Vertex::new([-0.5, -0.5, 0.5], MAGENTA),
                Vertex::new([0.5, -0.5, 0.5], MAGENTA),
                Vertex::new([-0.5, 0.5, 0.5], MAGENTA),
                // top
                Vertex::new([-0.5, 0.5, -0.5], YELLOW),
                Vertex::new([0.5, 0.5, 0.5], YELLOW),
                Vertex::new([0.5, 0.5, -0.5], YELLOW),
                Vertex::new([-0.5, 0.5, 0.5], YELLOW),
                // bottom
                Vertex::new([0.5, -0.5, 0.5], BLACK),
                Vertex::new([-0.5, -0.5, -0.5], BLACK),
                Vertex::new([0.5, -0.5, -0.5], BLACK),
                Vertex::new([-0.5, -0.5, 0.5], BLACK),
            ];

            let vertices_as_bytes = std::slice::from_raw_parts(
                (&vertices as *const _) as *const u8,
                std::mem::size_of_val(&vertices),
            );

            let vertex_buffers = create_default_buffer(&device, &list, vertices_as_bytes)?;

            let vertex_buffer_view = D3D12_VERTEX_BUFFER_VIEW {
                buffer_location: vertex_buffers.gpu_buffer.GetGPUVirtualAddress(),
                stride_in_bytes: std::mem::size_of::<Vertex>() as _,
                size_in_bytes: vertices_as_bytes.len() as _,
            };

            (
                vertex_buffers.gpu_buffer,
                vertex_buffer_view,
                vertex_buffers.upload_buffer,
            )
        };

        let (indices_buffer, indices_buffer_view, _indicies_upload_buffer) = unsafe {
            // Vertex indicies which form the two triangles:
            let indices: [u32; 36] = [
                // front
                0, 1, 2, // first triangle
                0, 3, 1, // second triangle
                // left
                4, 5, 6, // first triangle
                4, 7, 5, // second triangle
                // right
                8, 9, 10, // first triangle
                8, 11, 9, // second triangle
                // back
                12, 13, 14, // first triangle
                12, 15, 13, // second triangle
                // top
                16, 17, 18, // first triangle
                16, 19, 17, // second triangle
                // bottom
                20, 21, 22, // first triangle
                20, 23, 21, // second triangle
            ];

            let indicies_as_bytes = std::slice::from_raw_parts(
                (&indices as *const _) as *const u8,
                std::mem::size_of_val(&indices),
            );

            let buffers = create_default_buffer(&device, &list, indicies_as_bytes)?;

            let view = D3D12_INDEX_BUFFER_VIEW {
                buffer_location: buffers.gpu_buffer.GetGPUVirtualAddress(),
                size_in_bytes: indicies_as_bytes.len() as _,
                format: DXGI_FORMAT::DXGI_FORMAT_R32_UINT,
            };

            (buffers.gpu_buffer, view, buffers.upload_buffer)
        };

        unsafe {
            list.Close().ok()?;
            let mut lists = [Some(list.cast::<ID3D12CommandList>()?)];
            queue.ExecuteCommandLists(lists.len() as _, lists.as_mut_ptr());
        }

        unsafe {
            queue.Signal(&fence, fence_value).ok()?;
            fence.SetEventOnCompletion(fence_value, fence_event).ok()?;
            WaitForSingleObjectEx(fence_event, 0xFFFFFFFF, false);
        }

        let win = Window {
            hwnd,
            factory,
            adapter,
            device,
            queue,
            // allocators,
            comp_device,
            swap_chain,
            current_frame,
            comp_target,
            comp_visual,
            rtv_desc_heap,
            rtv_desc_size,
            back_buffers,
            depth_stencil_heap,
            depth_stencil_buffer,
            root_signature,
            // list,
            pipeline_state,
            vertex_shader,
            pixel_shader,
            viewport,
            scissor,
            vertex_buffer,
            vertex_buffer_view,
            indices_buffer,
            indices_buffer_view,
            // constant_buffer_heaps,
            // constant_buffers,
            camera,
            frame_resources,
            fence,
            fence_value,
            fence_event,
        };

        // Temporary upload buffers _indicies_upload_buffer, and
        // _vertex_buffer_upload can now be destroyed.

        // End of resource initialization -------------------------------

        Ok(win)
    }

    fn populate_command_list(&mut self) -> ::windows::Result<()> {
        unsafe {
            // Get the current backbuffer on which to draw
            let frame_resource = &self.frame_resources[self.current_frame];
            let current_back_buffer = &self.back_buffers[self.current_frame];
            let allocator = &frame_resource.allocator;
            let list = &frame_resource.list;
            let rtv = {
                let mut ptr = self.rtv_desc_heap.GetCPUDescriptorHandleForHeapStart();
                ptr.ptr += self.rtv_desc_size * self.current_frame;
                ptr
            };
            let dsv = self.depth_stencil_heap.GetCPUDescriptorHandleForHeapStart();

            // Reset allocator
            allocator.Reset().ok()?;

            // Reset list
            list.Reset(allocator, &self.pipeline_state).ok()?;

            // Set root signature, viewport and scissor rect
            list.SetGraphicsRootSignature(&self.root_signature);
            list.RSSetViewports(1, &self.viewport);
            list.RSSetScissorRects(1, &self.scissor);

            // Direct the draw commands to the render target resource
            list.ResourceBarrier(
                1,
                &cd3dx12_resource_barrier_transition(
                    current_back_buffer,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET,
                    None,
                    None,
                ),
            );
            list.ClearDepthStencilView(
                &dsv,
                D3D12_CLEAR_FLAGS::from(
                    D3D12_CLEAR_FLAGS::D3D12_CLEAR_FLAG_DEPTH.0
                        | D3D12_CLEAR_FLAGS::D3D12_CLEAR_FLAG_STENCIL.0,
                ),
                1.0,
                0,
                0,
                null_mut(),
            );
            list.OMSetRenderTargets(1, &rtv, false, &dsv);

            list.ClearRenderTargetView(rtv, [1.0f32, 0.2, 0.4, 0.5].as_ptr(), 0, null_mut());
            list.IASetPrimitiveTopology(
                D3D_PRIMITIVE_TOPOLOGY::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            );
            list.IASetIndexBuffer(&self.indices_buffer_view);
            list.IASetVertexBuffers(0, 1, &self.vertex_buffer_view);
            list.SetGraphicsRootConstantBufferView(
                0,
                self.frame_resources[self.current_frame]
                    .scene_cb
                    .gpu_virtual_address(),
            );
            list.SetGraphicsRootConstantBufferView(
                1,
                self.frame_resources[self.current_frame]
                    .object_cb
                    .gpu_virtual_address(),
            );
            list.DrawIndexedInstanced(36, 1, 0, 0, 0);

            // Set render target to be presentable
            list.ResourceBarrier(
                1,
                &cd3dx12_resource_barrier_transition(
                    current_back_buffer,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT,
                    None,
                    None,
                ),
            );

            // Close list
            list.Close().ok()?;
            Ok(())
        }
    }

    fn update(&mut self) -> windows::Result<()> {
        let frame = self.frame_resources[self.current_frame].borrow_mut();
        frame.update_constant_buffers(&self.camera);

        Ok(())
    }

    fn frame_next(&mut self) -> windows::Result<()> {
        self.current_frame = unsafe { self.swap_chain.GetCurrentBackBufferIndex() as _ };
        let frame = self.frame_resources[self.current_frame].borrow_mut();

        // Before update, ensure previous frame resource is done
        unsafe {
            let last_completed_fence = self.fence.GetCompletedValue();
            if frame.fence_value > last_completed_fence {
                self.fence
                    .SetEventOnCompletion(frame.fence_value, self.fence_event)
                    .ok()?;
                println!("Waiting for a frame... {}", self.current_frame);
                WaitForSingleObjectEx(self.fence_event, 0xFFFFFFFF, false);
            }
        }
        Ok(())
    }

    fn frame_done(&mut self) -> windows::Result<()> {
        let frame = self.frame_resources[self.current_frame].borrow_mut();

        // Signal and increment the fence value.
        frame.fence_value = self.fence_value;
        unsafe {
            self.queue.Signal(&self.fence, self.fence_value).ok()?;
        }
        self.fence_value += 1;
        Ok(())
    }

    fn render(&mut self) -> windows::Result<()> {
        self.populate_command_list()?;
        let frame_resource = &self.frame_resources[self.current_frame];
        unsafe {
            let mut lists = [Some(frame_resource.list.cast::<ID3D12CommandList>()?)];
            self.queue
                .ExecuteCommandLists(lists.len() as _, lists.as_mut_ptr());
            self.swap_chain.Present(1, 0).ok()?;
        }
        self.update()?;
        Ok(())
    }

    pub fn frame(&mut self) -> windows::Result<()> {
        // TODO: This seems really crappy and error prone
        self.frame_next()?;
        self.update()?;
        self.render()?;
        self.frame_done()?;
        Ok(())
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.camera.rotate_yaw(dx * 0.005);
        self.camera.rotate_pitch(dy * 0.005);
        self.frame().unwrap();
    }
}

static mut WINDOW: Option<Window> = None;

const fn get_xy(lparam: LPARAM) -> POINT {
    POINT {
        x: ((lparam.0 as i32) & (u16::MAX as i32)) as i16 as i32,
        y: ((lparam.0 as i32) >> 16) as _,
    }
}

const fn delta_xy(last: POINT, next: POINT) -> POINT {
    POINT {
        x: next.x - last.x,
        y: next.y - last.y,
    }
}

/// Main message loop for the window
extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    static mut LAST_POS: POINT = POINT { x: 0, y: 0 };
    static mut GRAB: bool = false;

    unsafe {
        match msg {
            WM_LBUTTONDOWN => {
                SetCapture(hwnd);
                LAST_POS = get_xy(lparam);
                GRAB = true;
                SetCursor(LoadCursorA(HINSTANCE(0), PSTR(IDC_SIZEALL as _)));
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                ReleaseCapture();
                SetCursor(LoadCursorA(HINSTANCE(0), PSTR(IDC_ARROW as _)));
                GRAB = false;
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if GRAB {
                    // Mouse delta from last point
                    let delta_pos = delta_xy(LAST_POS, get_xy(lparam));
                    if let Some(window) = WINDOW.as_mut() {
                        window.pan(delta_pos.x as _, delta_pos.y as _);
                    }

                    LAST_POS = get_xy(lparam);
                }
                LRESULT(0)
            }
            WM_PAINT => {
                if let Some(window) = WINDOW.as_mut() {
                    window.frame().unwrap();
                }
                ValidateRect(hwnd, std::ptr::null());
                LRESULT(0)
            }
            WM_DESTROY => {
                WINDOW = None;
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcA(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() {
    unsafe {
        // SetProcessDpiAwareness(PROCESS_DPI_AWARENESS::PROCESS_PER_MONITOR_DPI_AWARE).unwrap();
        let instance = HINSTANCE(GetModuleHandleA(PSTR(null_mut())));
        let cursor = LoadCursorA(HINSTANCE(0), PSTR(IDC_ARROW as _));
        let cls = WNDCLASSA {
            style: WNDCLASS_STYLES::CS_HREDRAW | WNDCLASS_STYLES::CS_VREDRAW,
            lpfn_wnd_proc: Some(wndproc),
            h_instance: instance,
            lpsz_class_name: PSTR(b"CompositionCls\0".as_ptr() as _),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_icon: HICON(0),
            h_cursor: cursor,
            hbr_background: HBRUSH(0),
            lpsz_menu_name: PSTR(null_mut()),
        };
        RegisterClassA(&cls);
        let hwnd = CreateWindowExA(
            WINDOWS_EX_STYLE::WS_EX_NOREDIRECTIONBITMAP as _,
            PSTR(b"CompositionCls\0".as_ptr() as _),
            PSTR(b"Constant Buffer example\0".as_ptr() as _),
            WINDOWS_STYLE::WS_OVERLAPPEDWINDOW | WINDOWS_STYLE::WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            HWND(0),
            HMENU(0),
            instance,
            0 as _,
        );
        if hwnd == HWND(0) {
            panic!("Failed to create window");
        }

        // Create the window
        WINDOW = Some(Window::new(hwnd).unwrap());

        let mut message = MSG::default();
        while GetMessageA(&mut message, HWND(0), 0, 0).into() {
            TranslateMessage(&mut message);
            DispatchMessageA(&mut message);
        }

        /*
        while message.message != WM_QUIT {
            if PeekMessageA(&mut message, HWND(0), 0, 0, PeekMessage_wRemoveMsg::PM_REMOVE).into() {
                TranslateMessage(&message);
                DispatchMessageA(&message);
            } else {
                if let Some(win) = WINDOW.as_mut() {
                    win.render().unwrap();
                }
            }
        }
        */
    }
}
