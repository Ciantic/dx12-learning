#![allow(unused_imports)]
/// CD3DX12 Helper functions from here:
/// https://github.com/microsoft/DirectX-Graphics-Samples/blob/master/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h
use bindings::{
    Windows::Win32::Direct3D12::*, Windows::Win32::Direct3DHlsl::*,
    Windows::Win32::DirectComposition::*, Windows::Win32::DisplayDevices::*,
    Windows::Win32::Dxgi::*, Windows::Win32::Gdi::*, Windows::Win32::HiDpi::*,
    Windows::Win32::KeyboardAndMouseInput::*, Windows::Win32::MenusAndResources::*,
    Windows::Win32::SystemServices::*, Windows::Win32::WindowsAndMessaging::*,
};
use directx_math::*;
use std::{convert::TryInto, ffi::CString, mem};
use std::{ffi::c_void, ptr::null_mut};
use windows::{Abi, Interface};

pub struct Buffers {
    pub upload_buffer: ID3D12Resource,
    pub gpu_buffer: ID3D12Resource,
}

/// Creates a gpu buffer from given data
///
/// Returns also upload buffer that must be kept alive until the command list is
/// executed.
pub fn create_default_buffer(
    device: &ID3D12Device,
    list: &ID3D12GraphicsCommandList,
    data: &[u8],
) -> ::windows::Result<Buffers> {
    let default_buffer = unsafe {
        let mut ptr: Option<ID3D12Resource> = None;
        device
            .CreateCommittedResource(
                &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_DEFAULT),
                D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                &cd3dx12_resource_desc_buffer(data.len() as _, None, None),
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COMMON,
                null_mut(),
                &ID3D12Resource::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)
    }?;

    let upload_buffer = unsafe {
        let mut ptr: Option<ID3D12Resource> = None;
        device
            .CreateCommittedResource(
                &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_UPLOAD),
                D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                &cd3dx12_resource_desc_buffer(data.len() as _, None, None),
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                null_mut(),
                &ID3D12Resource::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)
    }?;

    unsafe {
        list.ResourceBarrier(
            1,
            &cd3dx12_resource_barrier_transition(
                &default_buffer,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COMMON,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                None,
            ),
        );
    }

    update_subresources_stack_alloc::<1>(
        &list,
        &default_buffer,
        &upload_buffer,
        0,
        0,
        &mut [D3D12_SUBRESOURCE_DATA {
            pData: data.as_ptr() as *mut _,
            RowPitch: data.len() as _,
            SlicePitch: data.len() as _,
            ..Default::default()
        }],
    )?;

    unsafe {
        list.ResourceBarrier(
            1,
            &cd3dx12_resource_barrier_transition(
                &default_buffer,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                None,
                None,
            ),
        );
    }
    Ok(Buffers {
        gpu_buffer: default_buffer,
        upload_buffer,
    })
}

// #[derive(Debug)]
// pub struct ConstantBuffer<T: Sized> {
//     upload_buffer: UploadBuffer<T>,
//     shader_visibility: D3D12_SHADER_VISIBILITY,
// }

// TODO: UploadBuffer but like array with MutIndex or Index impl

#[derive(Debug)]
pub struct UploadBuffer<T: Sized> {
    buffer: ID3D12Resource,
    aligned_size: usize,
    gpu_memory_ptr: *mut T,
}

impl<T: Sized> UploadBuffer<T> {
    pub fn new(device: &ID3D12Device, init_data: &T) -> ::windows::Result<UploadBuffer<T>> {
        unsafe {
            let value_size = std::mem::size_of::<T>();
            let aligned_size = (value_size + 255) & !255;

            // Generic way to create upload buffer and get address:
            let mut ptr: Option<ID3D12Resource> = None;
            let buffer = device
                .CreateCommittedResource(
                    &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_UPLOAD),
                    D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                    &cd3dx12_resource_desc_buffer(aligned_size as _, None, None),
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                    std::ptr::null(),
                    &ID3D12Resource::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
                .expect("Unable to create constant buffer resource");

            // Notice that the memory location is left mapped
            let mut gpu_memory_ptr = null_mut::<T>();
            buffer
                .Map(
                    0,
                    &D3D12_RANGE { Begin: 0, End: 0 },
                    &mut gpu_memory_ptr as *mut *mut _ as *mut *mut _,
                )
                .ok()
                .expect("Unable to get memory location for constant buffer");

            std::ptr::copy_nonoverlapping(init_data, gpu_memory_ptr, 1);

            Ok(UploadBuffer {
                aligned_size,
                buffer,
                gpu_memory_ptr,
            })
        }
    }

    pub fn update(&mut self, value: &T) {
        unsafe {
            std::ptr::copy_nonoverlapping(value, self.gpu_memory_ptr, 1);
        }
    }

    pub fn gpu_virtual_address(&self) -> u64 {
        unsafe { self.buffer.GetGPUVirtualAddress() }
    }

    pub fn create_constant_buffer_view(
        &self,
        device: &ID3D12Device,
        cbv_heap: &ID3D12DescriptorHeap,
    ) {
        // TODO: Should I instead create and output ID3D12DescriptorHeap?
        unsafe {
            device.CreateConstantBufferView(
                &D3D12_CONSTANT_BUFFER_VIEW_DESC {
                    BufferLocation: self.gpu_virtual_address(),
                    SizeInBytes: self.aligned_size as _,
                },
                cbv_heap.GetCPUDescriptorHandleForHeapStart(),
            );
        }
    }
}

impl<T> Drop for UploadBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            self.buffer.Unmap(0, std::ptr::null());
        }
    }
}

pub fn create_upload_buffer(
    device: &ID3D12Device,
    data: &[u8],
) -> ::windows::Result<ID3D12Resource> {
    unsafe {
        let props = D3D12_HEAP_PROPERTIES {
            Type: D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_UPLOAD,
            CPUPageProperty: D3D12_CPU_PAGE_PROPERTY::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            CreationNodeMask: 1,
            VisibleNodeMask: 1,
            MemoryPoolPreference: D3D12_MEMORY_POOL::D3D12_MEMORY_POOL_UNKNOWN,
        };
        let desc = D3D12_RESOURCE_DESC {
            Alignment: 0,
            Flags: D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_NONE,
            Dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER,
            DepthOrArraySize: 1,
            Format: DXGI_FORMAT::DXGI_FORMAT_UNKNOWN,
            Height: 1,
            Width: data.len() as u64,
            Layout: D3D12_TEXTURE_LAYOUT::D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
        };
        let mut ptr: Option<ID3D12Resource> = None;
        let resource = device
            .CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                null_mut(),
                &ID3D12Resource::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)?;

        let mut gpu_data: *mut u8 = null_mut();
        resource
            .Map(
                0,
                &D3D12_RANGE { Begin: 0, End: 0 },
                &mut gpu_data as *mut *mut _ as *mut *mut _,
            )
            .ok()?;

        if gpu_data.is_null() {
            panic!("Failed to map");
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), gpu_data, data.len());

        // Debug, if you want to see what was copied
        // let gpu_slice = std::slice::from_raw_parts(gpu_triangle, 3);
        // println!("{:?}", cpu_triangle);
        // println!("{:?}", gpu_slice);

        resource.Unmap(0, null_mut());
        Ok(resource)
    }
}

pub fn cd3dx12_heap_properties_with_type(heap_type: D3D12_HEAP_TYPE) -> D3D12_HEAP_PROPERTIES {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L423-L433
    D3D12_HEAP_PROPERTIES {
        Type: heap_type,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL::D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    }
}

pub const fn cd3dx12_depth_stencil_desc_default() -> D3D12_DEPTH_STENCIL_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L177-L189
    D3D12_DEPTH_STENCIL_DESC {
        DepthEnable: BOOL(1),
        DepthWriteMask: D3D12_DEPTH_WRITE_MASK::D3D12_DEPTH_WRITE_MASK_ALL,
        DepthFunc: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_LESS,
        StencilEnable: BOOL(0),
        StencilReadMask: D3D12_DEFAULT_STENCIL_READ_MASK as _,
        StencilWriteMask: D3D12_DEFAULT_STENCIL_WRITE_MASK as _,
        FrontFace: D3D12_DEPTH_STENCILOP_DESC {
            StencilDepthFailOp: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            StencilFailOp: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            StencilPassOp: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            StencilFunc: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_ALWAYS,
        },
        BackFace: D3D12_DEPTH_STENCILOP_DESC {
            StencilDepthFailOp: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            StencilFailOp: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            StencilPassOp: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            StencilFunc: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_ALWAYS,
        },
    }
}

pub fn cd3dx12_blend_desc_default() -> D3D12_BLEND_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L323-L338
    D3D12_BLEND_DESC {
        AlphaToCoverageEnable: BOOL(0),
        IndependentBlendEnable: BOOL(0),
        RenderTarget: (0..D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT)
            .map(|_| D3D12_RENDER_TARGET_BLEND_DESC {
                BlendEnable: false.into(),
                LogicOpEnable: false.into(),
                DestBlend: D3D12_BLEND::D3D12_BLEND_ZERO,
                SrcBlend: D3D12_BLEND::D3D12_BLEND_ZERO,
                DestBlendAlpha: D3D12_BLEND::D3D12_BLEND_ONE,
                SrcBlendAlpha: D3D12_BLEND::D3D12_BLEND_ONE,
                BlendOp: D3D12_BLEND_OP::D3D12_BLEND_OP_ADD,
                LogicOp: D3D12_LOGIC_OP::D3D12_LOGIC_OP_NOOP,
                BlendOpAlpha: D3D12_BLEND_OP::D3D12_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE::D3D12_COLOR_WRITE_ENABLE_ALL.0
                    as _,
            })
            .collect::<Vec<_>>()
            .as_slice()
            .try_into()
            .unwrap(),
    }
}

pub fn cd3dx12_rasterizer_desc_default() -> D3D12_RASTERIZER_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L349-L359
    D3D12_RASTERIZER_DESC {
        FillMode: D3D12_FILL_MODE::D3D12_FILL_MODE_SOLID,
        CullMode: D3D12_CULL_MODE::D3D12_CULL_MODE_BACK,
        FrontCounterClockwise: false.into(),
        DepthBias: D3D12_DEFAULT_DEPTH_BIAS as _,
        DepthBiasClamp: D3D12_DEFAULT_DEPTH_BIAS_CLAMP,
        SlopeScaledDepthBias: D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS,
        DepthClipEnable: true.into(),
        MultisampleEnable: false.into(),
        AntialiasedLineEnable: false.into(),
        ForcedSampleCount: 0,
        ConservativeRaster:
            D3D12_CONSERVATIVE_RASTERIZATION_MODE::D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
    }
}

pub fn cd3dx12_resource_desc_buffer(
    width: u64,
    flags: Option<D3D12_RESOURCE_FLAGS>,
    alignment: Option<u64>,
) -> D3D12_RESOURCE_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/master/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L1754-L1756
    // Order follows the C++ function call order
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: alignment.unwrap_or(0),
        Width: width,
        DepthOrArraySize: 1,
        Height: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT::DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT::D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: flags.unwrap_or(D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_NONE),
    }
}

pub fn cd3dx12_resource_desc_tex2d(
    format: DXGI_FORMAT,
    width: u64,
    height: u32,
    array_size: Option<u16>,
    mip_levels: Option<u16>,
    sample_count: Option<u32>,
    sample_quality: Option<u32>,
    flags: Option<D3D12_RESOURCE_FLAGS>,
    layout: Option<D3D12_TEXTURE_LAYOUT>,
    alignment: Option<u64>,
) -> D3D12_RESOURCE_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L1773-L1787
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: alignment.unwrap_or(0),
        Width: width,
        DepthOrArraySize: array_size.unwrap_or(1),
        Height: height,
        MipLevels: mip_levels.unwrap_or(0),
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: sample_count.unwrap_or(1),
            Quality: sample_quality.unwrap_or(0),
        },
        Layout: layout.unwrap_or(D3D12_TEXTURE_LAYOUT::D3D12_TEXTURE_LAYOUT_UNKNOWN),
        Flags: flags.unwrap_or(D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_NONE),
    }
}

pub fn cd3dx12_resource_barrier_transition(
    resource: &ID3D12Resource,
    state_before: D3D12_RESOURCE_STATES,
    state_after: D3D12_RESOURCE_STATES,
    subresource: Option<u32>,
    flags: Option<D3D12_RESOURCE_BARRIER_FLAGS>,
) -> D3D12_RESOURCE_BARRIER {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L728-L744
    let subresource = subresource.unwrap_or(D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES);
    let flags = flags.unwrap_or(D3D12_RESOURCE_BARRIER_FLAGS::D3D12_RESOURCE_BARRIER_FLAG_NONE);

    let mut barrier = D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: flags,
        ..unsafe { std::mem::zeroed() }
    };
    barrier.Anonymous.Transition.Subresource = subresource;
    barrier.Anonymous.Transition.pResource = resource.abi();
    barrier.Anonymous.Transition.StateBefore = state_before;
    barrier.Anonymous.Transition.StateAfter = state_after;
    barrier
}

pub fn cd3dx12_texture_copy_location_sub(
    res: &ID3D12Resource,
    sub: u32,
) -> D3D12_TEXTURE_COPY_LOCATION {
    let mut res = D3D12_TEXTURE_COPY_LOCATION {
        // TODO: This should be pointer, can I get rid of clone?
        pResource: Some(res.clone()),
        Type: D3D12_TEXTURE_COPY_TYPE::D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        ..unsafe { std::mem::zeroed() }
    };

    res.Anonymous.PlacedFootprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
        ..unsafe { std::mem::zeroed() }
    };
    res.Anonymous.SubresourceIndex = sub;
    res
}

pub fn cd3dx12_texture_copy_location_footprint(
    res: &ID3D12Resource,
    footprint: &D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
) -> D3D12_TEXTURE_COPY_LOCATION {
    let mut res = D3D12_TEXTURE_COPY_LOCATION {
        // TODO: This should be pointer, can I get rid of clone?
        pResource: Some(res.clone()),
        Type: D3D12_TEXTURE_COPY_TYPE::D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        ..unsafe { std::mem::zeroed() }
    };
    res.Anonymous.PlacedFootprint = footprint.clone();
    res
}

/// WinAPI equivalent of SIZE_T(-1)
///
/// This is also bitwise not zero !0 or (in C++ ~0), not sure why the hell it's
/// written as SIZE_T(-1)
const SIZE_T_MINUS1: usize = usize::MAX;

/// Update subresources
//
/// This is mimicking stack allocation implementation
pub fn update_subresources_stack_alloc<const MAX_SUBRESOURCES: usize>(
    list: &ID3D12GraphicsCommandList,
    dest_resource: &ID3D12Resource,
    intermediate: &ID3D12Resource,
    intermediate_offset: u64,
    first_subresource: u32,
    p_src_data: &mut [D3D12_SUBRESOURCE_DATA; MAX_SUBRESOURCES],
) -> ::windows::Result<u64> {
    update_subresources_stack_alloc_raw::<MAX_SUBRESOURCES>(
        list,
        dest_resource,
        intermediate,
        intermediate_offset,
        first_subresource,
        p_src_data.len() as _,
        p_src_data.as_mut_ptr(),
    )
}

/// Update subresources
//
/// This is mimicking stack allocation implementation
fn update_subresources_stack_alloc_raw<const MAX_SUBRESOURCES: usize>(
    list: &ID3D12GraphicsCommandList,
    dest_resource: &ID3D12Resource,
    intermediate: &ID3D12Resource,
    intermediate_offset: u64,
    first_subresource: u32,
    num_subresources: u32,
    p_src_data: *mut D3D12_SUBRESOURCE_DATA,
) -> ::windows::Result<u64> {
    // Stack alloc implementation
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L2118-L2140
    let src_data = unsafe { std::slice::from_raw_parts_mut(p_src_data, num_subresources as _) };
    let mut required_size = 0;
    let mut layouts = [D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); MAX_SUBRESOURCES];
    let mut num_rows = [0; MAX_SUBRESOURCES];
    let mut row_sizes_in_bytes = [0; MAX_SUBRESOURCES];
    let desc = unsafe { dest_resource.GetDesc() };
    unsafe {
        let dest_device = {
            let mut ptr: Option<ID3D12Device> = None;
            dest_resource
                .GetDevice(&ID3D12Device::IID, ptr.set_abi())
                .and_some(ptr)
        }?;
        dest_device.GetCopyableFootprints(
            &desc,
            first_subresource,
            num_subresources as _,
            intermediate_offset,
            layouts.as_mut_ptr(),
            num_rows.as_mut_ptr(),
            row_sizes_in_bytes.as_mut_ptr(),
            &mut required_size,
        );
    }

    // UpdateSubresources main implementation
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L2036-L2076

    // Minor validation
    let intermediate_desc = unsafe { intermediate.GetDesc() };
    let dest_desc = unsafe { dest_resource.GetDesc() };
    if intermediate_desc.Dimension != D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER
        || intermediate_desc.Width < (required_size + layouts[0].Offset)
        || required_size > (SIZE_T_MINUS1 as u64)
        || (dest_desc.Dimension == D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER
            && (first_subresource != 0 || num_subresources != 1))
    {
        return Ok(0); // TODO: Is this actually a failure?
    }

    let mut p_data = null_mut();

    unsafe { intermediate.Map(0, null_mut(), &mut p_data) }.ok()?;

    for i in 0..(num_subresources as usize) {
        if row_sizes_in_bytes[i] > (SIZE_T_MINUS1 as u64) {
            return Ok(0); // TODO: Is this actually a failure?
        }

        let mut dest_data = D3D12_MEMCPY_DEST {
            pData: ((p_data as u64) + layouts[i].Offset) as *mut _,
            RowPitch: layouts[i].Footprint.RowPitch as _,
            SlicePitch: (layouts[i].Footprint.RowPitch as usize) * (num_rows[i] as usize),
        };
        memcpy_subresource(
            &mut dest_data,
            &src_data[i],
            row_sizes_in_bytes[i] as _,
            num_rows[i],
            layouts[i].Footprint.Depth,
        )
    }
    unsafe {
        intermediate.Unmap(0, null_mut());
    }

    if dest_desc.Dimension == D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER {
        unsafe {
            list.CopyBufferRegion(
                dest_resource,
                0,
                intermediate,
                layouts[0].Offset,
                layouts[0].Footprint.Width as _,
            );
        }
    } else {
        // TODO: Never tested
        for i in 0..(num_subresources as usize) {
            let dst =
                cd3dx12_texture_copy_location_sub(&dest_resource, (i as u32) + first_subresource);
            let src = cd3dx12_texture_copy_location_footprint(&intermediate, &layouts[i]);
            unsafe {
                list.CopyTextureRegion(&dst, 0, 0, 0, &src, null_mut());
            }
        }
    }

    return Ok(required_size);
}

/// Row-by-row memcpy
pub fn memcpy_subresource(
    dest: *mut D3D12_MEMCPY_DEST,
    src: *const D3D12_SUBRESOURCE_DATA,
    row_size_in_bytes: usize,
    num_rows: u32,
    num_slices: u32,
) {
    // TODO: Tested only with num_rows = 1, num_slices = 1
    // unsafe {
    //     println!("dest {:?}", *dest);
    //     println!("src {:?}", *src);
    //     println!("num_rows {}", num_rows);
    //     println!("num_slices {}", num_slices);
    //     println!("row_size_in_bytes {}", row_size_in_bytes);
    // }
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L1983-L2001
    for z in 0..(num_slices as usize) {
        unsafe {
            let dest_slice = ((*dest).pData as usize) + (*dest).SlicePitch * z;
            let src_slice = ((*src).pData as usize) + ((*src).SlicePitch as usize) * z;
            for y in 0..(num_rows as usize) {
                std::ptr::copy_nonoverlapping(
                    (src_slice + ((*src).RowPitch as usize) * y) as *const u8,
                    (dest_slice + (*dest).RowPitch * y) as *mut u8,
                    row_size_in_bytes,
                );
            }
        }
    }

    // unsafe {
    //     #[derive(Debug)]
    //     #[repr(C)]
    //     struct Vertex {
    //         position: [f32; 3],
    //         color: [f32; 4],
    //     }

    //     let src_slice_view = std::slice::from_raw_parts((*src).p_data as *const Vertex, 3);
    //     let dest_slice_view = std::slice::from_raw_parts((*dest).p_data as *const Vertex, 3);
    //     println!("{:?}", src_slice_view);
    //     println!("{:?}", dest_slice_view);
    // }
}
