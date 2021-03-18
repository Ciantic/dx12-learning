#![allow(unused_imports)]

/// Tutorials followed:
///
/// https://github.com/microsoft/DirectX-Graphics-Samples/blob/master/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/D3D12HelloTriangle.cpp
///
/// https://www.braynzarsoft.net/viewtutorial/q16390-directx-12-index-buffers
///
use bindings::{
    windows::win32::direct3d11::*, windows::win32::direct3d12::*, windows::win32::direct3d_hlsl::*,
    windows::win32::direct_composition::*, windows::win32::display_devices::*,
    windows::win32::dxgi::*, windows::win32::gdi::*, windows::win32::menus_and_resources::*,
    windows::win32::system_services::*, windows::win32::windows_and_messaging::*,
};
use std::{convert::TryInto, ffi::CString};
use std::{ffi::c_void, ptr::null_mut};
use windows::{Abi, Interface};

pub struct Buffers {
    upload_buffer: ID3D12Resource,
    gpu_buffer: ID3D12Resource,
}

pub fn create_default_buffer(
    device: &ID3D12Device,
    list: &ID3D12GraphicsCommandList,
    init_data: *mut c_void,
    byte_size: usize,
) -> ::windows::Result<Buffers> {
    let default_buffer = unsafe {
        let mut ptr: Option<ID3D12Resource> = None;
        device
            .CreateCommittedResource(
                &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_DEFAULT),
                D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                &cd3dx12_resource_desc_buffer(byte_size as _, None, None),
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
                &cd3dx12_resource_desc_buffer(byte_size as _, None, None),
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                null_mut(),
                &ID3D12Resource::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)
    }?;

    let sub_data = D3D12_SUBRESOURCE_DATA {
        p_data: init_data,
        row_pitch: byte_size as _,
        slice_pitch: byte_size as _,
        ..Default::default()
    };

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
    /*
    TODO: update_subresources(list, default_buffer.Get(), upload_buffer.Get(), 0, 0, 1, &sub_data);
    */

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
    todo!()
}

pub fn cd3dx12_heap_properties_with_type(t: D3D12_HEAP_TYPE) -> D3D12_HEAP_PROPERTIES {
    D3D12_HEAP_PROPERTIES {
        r#type: t,
        cpu_page_property: D3D12_CPU_PAGE_PROPERTY::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        memory_pool_preference: D3D12_MEMORY_POOL::D3D12_MEMORY_POOL_UNKNOWN,
        creation_node_mask: 1,
        visible_node_mask: 1,
    }
}

pub const fn cd3dx12_depth_stencil_desc_default() -> D3D12_DEPTH_STENCIL_DESC {
    D3D12_DEPTH_STENCIL_DESC {
        depth_enable: BOOL(1),
        depth_write_mask: D3D12_DEPTH_WRITE_MASK::D3D12_DEPTH_WRITE_MASK_ALL,
        depth_func: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_LESS,
        stencil_enable: BOOL(0),
        stencil_read_mask: D3D12_DEFAULT_STENCIL_READ_MASK as _,
        stencil_write_mask: D3D12_DEFAULT_STENCIL_WRITE_MASK as _,
        front_face: D3D12_DEPTH_STENCILOP_DESC {
            stencil_depth_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_pass_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_func: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_ALWAYS,
        },
        back_face: D3D12_DEPTH_STENCILOP_DESC {
            stencil_depth_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_pass_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_func: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_ALWAYS,
        },
    }
}

pub fn cd3dx12_blend_desc_default() -> D3D12_BLEND_DESC {
    D3D12_BLEND_DESC {
        alpha_to_coverage_enable: BOOL(0),
        independent_blend_enable: BOOL(0),
        render_target: (0..D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT)
            .map(|_| D3D12_RENDER_TARGET_BLEND_DESC {
                blend_enable: false.into(),
                logic_op_enable: false.into(),
                dest_blend: D3D12_BLEND::D3D12_BLEND_ZERO,
                src_blend: D3D12_BLEND::D3D12_BLEND_ZERO,
                dest_blend_alpha: D3D12_BLEND::D3D12_BLEND_ONE,
                src_blend_alpha: D3D12_BLEND::D3D12_BLEND_ONE,
                blend_op: D3D12_BLEND_OP::D3D12_BLEND_OP_ADD,
                logic_op: D3D12_LOGIC_OP::D3D12_LOGIC_OP_NOOP,
                blend_op_alpha: D3D12_BLEND_OP::D3D12_BLEND_OP_ADD,
                render_target_write_mask: D3D12_COLOR_WRITE_ENABLE::D3D12_COLOR_WRITE_ENABLE_ALL.0
                    as _,
            })
            .collect::<Vec<_>>()
            .as_slice()
            .try_into()
            .unwrap(),
    }
}

///
///
/// https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L349-L359
pub fn cd3dx12_rasterizer_desc_default() -> D3D12_RASTERIZER_DESC {
    D3D12_RASTERIZER_DESC {
        fill_mode: D3D12_FILL_MODE::D3D12_FILL_MODE_SOLID,
        cull_mode: D3D12_CULL_MODE::D3D12_CULL_MODE_BACK,
        front_counter_clockwise: false.into(),
        depth_bias: D3D12_DEFAULT_DEPTH_BIAS as _,
        depth_bias_clamp: D3D12_DEFAULT_DEPTH_BIAS_CLAMP,
        slope_scaled_depth_bias: D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS,
        depth_clip_enable: true.into(),
        multisample_enable: false.into(),
        antialiased_line_enable: false.into(),
        forced_sample_count: 0,
        conservative_raster:
            D3D12_CONSERVATIVE_RASTERIZATION_MODE::D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
    }
}

///
///
/// https://github.com/microsoft/DirectX-Graphics-Samples/blob/master/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L1754-L1756
pub fn cd3dx12_resource_desc_buffer(
    width: u64,
    flags: Option<D3D12_RESOURCE_FLAGS>,
    alignment: Option<u64>,
) -> D3D12_RESOURCE_DESC {
    // Order follows the C++ function call order
    D3D12_RESOURCE_DESC {
        dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER,
        alignment: alignment.unwrap_or(0),
        width,
        depth_or_array_size: 1,
        height: 1,
        mip_levels: 1,
        format: DXGI_FORMAT::DXGI_FORMAT_UNKNOWN,
        sample_desc: DXGI_SAMPLE_DESC {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT::D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        flags: flags.unwrap_or(D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_NONE),
    }
}

pub fn cd3dx12_resource_barrier_transition(
    resource: &ID3D12Resource,
    state_before: D3D12_RESOURCE_STATES,
    state_after: D3D12_RESOURCE_STATES,
    subresource: Option<u32>,
    flags: Option<D3D12_RESOURCE_BARRIER_FLAGS>,
) -> D3D12_RESOURCE_BARRIER {
    let subresource = subresource.unwrap_or(D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES);
    let flags = flags.unwrap_or(D3D12_RESOURCE_BARRIER_FLAGS::D3D12_RESOURCE_BARRIER_FLAG_NONE);

    let mut barrier = D3D12_RESOURCE_BARRIER {
        r#type: D3D12_RESOURCE_BARRIER_TYPE::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        flags,
        ..unsafe { std::mem::zeroed() }
    };
    barrier.anonymous.transition.subresource = subresource;
    barrier.anonymous.transition.p_resource = resource.abi();
    barrier.anonymous.transition.state_before = state_before;
    barrier.anonymous.transition.state_after = state_after;
    barrier
}

/*
// https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L2081-L2113
// Heap-allocating UpdateSubresources implementation
inline UINT64 UpdateSubresources(
    _In_ ID3D12GraphicsCommandList* pCmdList,
    _In_ ID3D12Resource* pDestinationResource,
    _In_ ID3D12Resource* pIntermediate,
    UINT64 IntermediateOffset,
    _In_range_(0,D3D12_REQ_SUBRESOURCES) UINT FirstSubresource,
    _In_range_(0,D3D12_REQ_SUBRESOURCES-FirstSubresource) UINT NumSubresources,
    _In_reads_(NumSubresources) const D3D12_SUBRESOURCE_DATA* pSrcData) noexcept
{
    UINT64 RequiredSize = 0;
    UINT64 MemToAlloc = static_cast<UINT64>(sizeof(D3D12_PLACED_SUBRESOURCE_FOOTPRINT) + sizeof(UINT) + sizeof(UINT64)) * NumSubresources;
    if (MemToAlloc > SIZE_MAX)
    {
       return 0;
    }
    void* pMem = HeapAlloc(GetProcessHeap(), 0, static_cast<SIZE_T>(MemToAlloc));
    if (pMem == nullptr)
    {
       return 0;
    }
    auto pLayouts = static_cast<D3D12_PLACED_SUBRESOURCE_FOOTPRINT*>(pMem);
    UINT64* pRowSizesInBytes = reinterpret_cast<UINT64*>(pLayouts + NumSubresources);
    UINT* pNumRows = reinterpret_cast<UINT*>(pRowSizesInBytes + NumSubresources);

    auto Desc = pDestinationResource->GetDesc();
    ID3D12Device* pDevice = nullptr;
    pDestinationResource->GetDevice(IID_ID3D12Device, reinterpret_cast<void**>(&pDevice));
    pDevice->GetCopyableFootprints(&Desc, FirstSubresource, NumSubresources, IntermediateOffset, pLayouts, pNumRows, pRowSizesInBytes, &RequiredSize);
    pDevice->Release();

    UINT64 Result = UpdateSubresources(pCmdList, pDestinationResource, pIntermediate, FirstSubresource, NumSubresources, RequiredSize, pLayouts, pNumRows, pRowSizesInBytes, pSrcData);
    HeapFree(GetProcessHeap(), 0, pMem);
    return Result;
}
*/
