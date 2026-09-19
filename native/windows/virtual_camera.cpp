//------------------------------------------------------------------------------
// DJI Live Bridge Camera — a DirectShow capture source for Windows.
//
// Apps such as TikTok LIVE Studio, OBS, Zoom and Discord enumerate DirectShow
// video input devices, so this is what makes the drone picture selectable as a
// camera. The filter itself holds no pipeline: it copies the newest frame the
// app published in shared memory (see shared_frame.h) and hands it downstream,
// showing a placeholder while the app is not streaming.
//
// Registered and unregistered by the installer through regsvr32.
//------------------------------------------------------------------------------

#include <streams.h>
#include <initguid.h>
#include <olectl.h>
#include <dvdmedia.h>

#include "shared_frame.h"

// {6F1D9A0C-6B2E-4F0B-9E2E-2C8B3F5A7D41}
DEFINE_GUID(CLSID_DjiLiveBridgeCamera,
    0x6f1d9a0c, 0x6b2e, 0x4f0b, 0x9e, 0x2e, 0x2c, 0x8b, 0x3f, 0x5a, 0x7d, 0x41);

static const WCHAR kFilterName[] = L"DJI Live Bridge Camera";

// The frame rate in 100-ns units, which is how DirectShow measures time.
static const REFERENCE_TIME kFrameDuration = UNITS / DJI_FRAME_FPS;

class DjiCameraStream;

//------------------------------------------------------------------------------
// Reader for the app's shared frame buffer.
//------------------------------------------------------------------------------
class SharedFrameReader {
public:
    ~SharedFrameReader() { Close(); }

    // Reconnects on demand: the app may start after the camera was opened.
    bool CopyLatest(BYTE* destination, long capacity) {
        if (!m_view && !Open()) {
            return false;
        }
        const DjiFrameHeader* header = reinterpret_cast<const DjiFrameHeader*>(m_view);
        if (header->magic != DJI_FRAME_MAGIC || header->width != DJI_FRAME_WIDTH ||
            header->height != DJI_FRAME_HEIGHT || capacity < DJI_FRAME_BYTES) {
            return false;
        }
        const BYTE* pixels = m_view + sizeof(DjiFrameHeader);
        // Retry briefly if the writer is mid-frame (odd sequence) or if it
        // replaced the frame while we copied.
        for (int attempt = 0; attempt < 8; ++attempt) {
            const uint32_t before = header->sequence;
            if (before == 0 || (before & 1u)) {
                Sleep(1);
                continue;
            }
            memcpy(destination, pixels, DJI_FRAME_BYTES);
            if (header->sequence == before) {
                m_lastSequence = before;
                return true;
            }
        }
        return false;
    }

    // True when the app has published at least one frame recently.
    bool Live() const { return m_view != nullptr && m_lastSequence != 0; }

private:
    bool Open() {
        m_mapping = OpenFileMappingW(FILE_MAP_READ, FALSE, DJI_FRAME_MAPPING_NAME);
        if (!m_mapping) {
            return false;
        }
        m_view = static_cast<BYTE*>(
            MapViewOfFile(m_mapping, FILE_MAP_READ, 0, 0, DJI_FRAME_MAPPING_BYTES));
        if (!m_view) {
            CloseHandle(m_mapping);
            m_mapping = nullptr;
            return false;
        }
        return true;
    }

    void Close() {
        if (m_view) {
            UnmapViewOfFile(m_view);
            m_view = nullptr;
        }
        if (m_mapping) {
            CloseHandle(m_mapping);
            m_mapping = nullptr;
        }
    }

    HANDLE m_mapping = nullptr;
    BYTE* m_view = nullptr;
    uint32_t m_lastSequence = 0;
};

//------------------------------------------------------------------------------
// The filter.
//------------------------------------------------------------------------------
class DjiCameraFilter : public CSource, public IAMFilterMiscFlags {
public:
    static CUnknown* WINAPI CreateInstance(LPUNKNOWN unknown, HRESULT* result);

    DECLARE_IUNKNOWN;
    STDMETHODIMP NonDelegatingQueryInterface(REFIID riid, void** ppv) override {
        if (riid == IID_IAMFilterMiscFlags) {
            return GetInterface(static_cast<IAMFilterMiscFlags*>(this), ppv);
        }
        return CSource::NonDelegatingQueryInterface(riid, ppv);
    }

    // Tells the graph this is a live source, so nothing tries to seek it.
    ULONG STDMETHODCALLTYPE GetMiscFlags() override { return AM_FILTER_MISC_FLAGS_IS_SOURCE; }

private:
    DjiCameraFilter(LPUNKNOWN unknown, HRESULT* result);
};

//------------------------------------------------------------------------------
// The output pin: produces one NV12 frame per tick.
//------------------------------------------------------------------------------
class DjiCameraStream : public CSourceStream, public IAMStreamConfig, public IKsPropertySet {
public:
    DjiCameraStream(HRESULT* result, DjiCameraFilter* filter);

    DECLARE_IUNKNOWN;
    STDMETHODIMP NonDelegatingQueryInterface(REFIID riid, void** ppv) override {
        if (riid == IID_IAMStreamConfig) {
            return GetInterface(static_cast<IAMStreamConfig*>(this), ppv);
        }
        if (riid == IID_IKsPropertySet) {
            return GetInterface(static_cast<IKsPropertySet*>(this), ppv);
        }
        return CSourceStream::NonDelegatingQueryInterface(riid, ppv);
    }

    // CSourceStream
    HRESULT FillBuffer(IMediaSample* sample) override;
    HRESULT DecideBufferSize(IMemAllocator* allocator, ALLOCATOR_PROPERTIES* request) override;
    HRESULT GetMediaType(int position, CMediaType* mediaType) override;
    HRESULT CheckMediaType(const CMediaType* mediaType) override;
    HRESULT OnThreadCreate() override;

    // IAMStreamConfig — one fixed format, which keeps capture apps happy.
    HRESULT STDMETHODCALLTYPE SetFormat(AM_MEDIA_TYPE* format) override;
    HRESULT STDMETHODCALLTYPE GetFormat(AM_MEDIA_TYPE** format) override;
    HRESULT STDMETHODCALLTYPE GetNumberOfCapabilities(int* count, int* size) override;
    HRESULT STDMETHODCALLTYPE GetStreamCaps(int index, AM_MEDIA_TYPE** format,
                                            BYTE* capabilities) override;

    // IKsPropertySet — capture apps ask for the pin category through this.
    HRESULT STDMETHODCALLTYPE Set(REFGUID guidPropSet, DWORD id, void* instanceData,
                                  DWORD instanceLength, void* propertyData,
                                  DWORD dataLength) override;
    HRESULT STDMETHODCALLTYPE Get(REFGUID guidPropSet, DWORD id, void* instanceData,
                                  DWORD instanceLength, void* propertyData, DWORD dataLength,
                                  DWORD* returned) override;
    HRESULT STDMETHODCALLTYPE QuerySupported(REFGUID guidPropSet, DWORD id,
                                             DWORD* typeSupport) override;

private:
    void FillMediaType(CMediaType* mediaType) const;
    void DrawPlaceholder(BYTE* destination) const;

    CCritSec m_lock;
    SharedFrameReader m_reader;
    REFERENCE_TIME m_nextFrameStart = 0;
};

//------------------------------------------------------------------------------

DjiCameraFilter::DjiCameraFilter(LPUNKNOWN unknown, HRESULT* result)
    : CSource(kFilterName, unknown, CLSID_DjiLiveBridgeCamera) {
    new DjiCameraStream(result, this); // CSource owns the pin
}

CUnknown* WINAPI DjiCameraFilter::CreateInstance(LPUNKNOWN unknown, HRESULT* result) {
    DjiCameraFilter* filter = new DjiCameraFilter(unknown, result);
    if (!filter && result) {
        *result = E_OUTOFMEMORY;
    }
    return filter;
}

DjiCameraStream::DjiCameraStream(HRESULT* result, DjiCameraFilter* filter)
    : CSourceStream(kFilterName, result, filter, L"Capture") {}

void DjiCameraStream::FillMediaType(CMediaType* mediaType) const {
    VIDEOINFOHEADER* info = reinterpret_cast<VIDEOINFOHEADER*>(
        mediaType->AllocFormatBuffer(sizeof(VIDEOINFOHEADER)));
    ZeroMemory(info, sizeof(VIDEOINFOHEADER));
    info->AvgTimePerFrame = kFrameDuration;
    info->bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    info->bmiHeader.biWidth = DJI_FRAME_WIDTH;
    info->bmiHeader.biHeight = DJI_FRAME_HEIGHT;
    info->bmiHeader.biPlanes = 1;
    info->bmiHeader.biBitCount = 12; // NV12
    info->bmiHeader.biCompression = MAKEFOURCC('N', 'V', '1', '2');
    info->bmiHeader.biSizeImage = DJI_FRAME_BYTES;
    SetRectEmpty(&info->rcSource);
    SetRectEmpty(&info->rcTarget);

    mediaType->SetType(&MEDIATYPE_Video);
    mediaType->SetFormatType(&FORMAT_VideoInfo);
    mediaType->SetTemporalCompression(FALSE);
    GUID subtype = MEDIASUBTYPE_NV12;
    mediaType->SetSubtype(&subtype);
    mediaType->SetSampleSize(DJI_FRAME_BYTES);
}

HRESULT DjiCameraStream::GetMediaType(int position, CMediaType* mediaType) {
    CAutoLock lock(m_pFilter->pStateLock());
    if (position < 0) {
        return E_INVALIDARG;
    }
    if (position > 0) {
        return VFW_S_NO_MORE_ITEMS;
    }
    FillMediaType(mediaType);
    return S_OK;
}

HRESULT DjiCameraStream::CheckMediaType(const CMediaType* mediaType) {
    if (*mediaType->Type() != MEDIATYPE_Video || *mediaType->Subtype() != MEDIASUBTYPE_NV12 ||
        *mediaType->FormatType() != FORMAT_VideoInfo || mediaType->Format() == nullptr) {
        return E_INVALIDARG;
    }
    const VIDEOINFOHEADER* info = reinterpret_cast<const VIDEOINFOHEADER*>(mediaType->Format());
    if (info->bmiHeader.biWidth != DJI_FRAME_WIDTH ||
        abs(info->bmiHeader.biHeight) != DJI_FRAME_HEIGHT) {
        return E_INVALIDARG;
    }
    return S_OK;
}

HRESULT DjiCameraStream::DecideBufferSize(IMemAllocator* allocator,
                                          ALLOCATOR_PROPERTIES* request) {
    CAutoLock lock(m_pFilter->pStateLock());
    request->cBuffers = 2;
    request->cbBuffer = DJI_FRAME_BYTES;

    ALLOCATOR_PROPERTIES actual;
    HRESULT result = allocator->SetProperties(request, &actual);
    if (FAILED(result)) {
        return result;
    }
    return actual.cbBuffer < request->cbBuffer ? E_FAIL : S_OK;
}

HRESULT DjiCameraStream::OnThreadCreate() {
    m_nextFrameStart = 0;
    return S_OK;
}

// A calm dark frame with a lighter band, so a user who selects the camera
// before starting the app sees something deliberate rather than noise.
void DjiCameraStream::DrawPlaceholder(BYTE* destination) const {
    const int lumaBytes = DJI_FRAME_WIDTH * DJI_FRAME_HEIGHT;
    memset(destination, 16, lumaBytes);                       // near-black luma
    memset(destination + lumaBytes, 128, lumaBytes / 2);      // neutral chroma
    const int bandTop = DJI_FRAME_HEIGHT / 2 - 2;
    memset(destination + bandTop * DJI_FRAME_WIDTH, 60, DJI_FRAME_WIDTH * 4);
}

HRESULT DjiCameraStream::FillBuffer(IMediaSample* sample) {
    BYTE* destination = nullptr;
    HRESULT result = sample->GetPointer(&destination);
    if (FAILED(result)) {
        return result;
    }
    const long capacity = sample->GetSize();

    {
        CAutoLock lock(&m_lock);
        if (!m_reader.CopyLatest(destination, capacity)) {
            DrawPlaceholder(destination);
        }
    }
    sample->SetActualDataLength(DJI_FRAME_BYTES);

    // Pace the graph ourselves: this is a live source with a fixed rate.
    REFERENCE_TIME start = m_nextFrameStart;
    REFERENCE_TIME end = start + kFrameDuration;
    m_nextFrameStart = end;
    sample->SetTime(&start, &end);
    sample->SetSyncPoint(TRUE);

    CRefTime now;
    m_pFilter->StreamTime(now);
    const REFERENCE_TIME wait = start - now;
    if (wait > 0) {
        Sleep(static_cast<DWORD>(wait / (UNITS / 1000)));
    }
    return S_OK;
}

HRESULT STDMETHODCALLTYPE DjiCameraStream::SetFormat(AM_MEDIA_TYPE* format) {
    // The camera offers exactly one format; accept only that one.
    if (format == nullptr) {
        return S_OK;
    }
    CMediaType requested(*format);
    return CheckMediaType(&requested) == S_OK ? S_OK : E_INVALIDARG;
}

HRESULT STDMETHODCALLTYPE DjiCameraStream::GetFormat(AM_MEDIA_TYPE** format) {
    if (format == nullptr) {
        return E_POINTER;
    }
    CMediaType mediaType;
    FillMediaType(&mediaType);
    *format = CreateMediaType(&mediaType);
    return *format ? S_OK : E_OUTOFMEMORY;
}

HRESULT STDMETHODCALLTYPE DjiCameraStream::GetNumberOfCapabilities(int* count, int* size) {
    if (count == nullptr || size == nullptr) {
        return E_POINTER;
    }
    *count = 1;
    *size = sizeof(VIDEO_STREAM_CONFIG_CAPS);
    return S_OK;
}

HRESULT STDMETHODCALLTYPE DjiCameraStream::GetStreamCaps(int index, AM_MEDIA_TYPE** format,
                                                         BYTE* capabilities) {
    if (index != 0) {
        return S_FALSE;
    }
    if (format == nullptr || capabilities == nullptr) {
        return E_POINTER;
    }
    HRESULT result = GetFormat(format);
    if (FAILED(result)) {
        return result;
    }

    VIDEO_STREAM_CONFIG_CAPS* caps = reinterpret_cast<VIDEO_STREAM_CONFIG_CAPS*>(capabilities);
    ZeroMemory(caps, sizeof(VIDEO_STREAM_CONFIG_CAPS));
    caps->guid = FORMAT_VideoInfo;
    caps->MinCroppingSize.cx = caps->MaxCroppingSize.cx = DJI_FRAME_WIDTH;
    caps->MinCroppingSize.cy = caps->MaxCroppingSize.cy = DJI_FRAME_HEIGHT;
    caps->MinOutputSize = caps->MaxOutputSize = caps->MinCroppingSize;
    caps->CropGranularityX = caps->CropGranularityY = 1;
    caps->OutputGranularityX = caps->OutputGranularityY = 1;
    caps->MinFrameInterval = caps->MaxFrameInterval = kFrameDuration;
    caps->MinBitsPerSecond = caps->MaxBitsPerSecond =
        static_cast<LONG>(DJI_FRAME_BYTES) * 8 * DJI_FRAME_FPS;
    return S_OK;
}

HRESULT STDMETHODCALLTYPE DjiCameraStream::Set(REFGUID, DWORD, void*, DWORD, void*, DWORD) {
    return E_NOTIMPL;
}

HRESULT STDMETHODCALLTYPE DjiCameraStream::Get(REFGUID guidPropSet, DWORD id, void* instanceData,
                                               DWORD instanceLength, void* propertyData,
                                               DWORD dataLength, DWORD* returned) {
    UNREFERENCED_PARAMETER(instanceData);
    UNREFERENCED_PARAMETER(instanceLength);
    if (guidPropSet != AMPROPSETID_Pin) {
        return E_PROP_SET_UNSUPPORTED;
    }
    if (id != AMPROPERTY_PIN_CATEGORY) {
        return E_PROP_ID_UNSUPPORTED;
    }
    if (propertyData == nullptr && returned == nullptr) {
        return E_POINTER;
    }
    if (returned) {
        *returned = sizeof(GUID);
    }
    if (propertyData == nullptr) {
        return S_OK;
    }
    if (dataLength < sizeof(GUID)) {
        return E_UNEXPECTED;
    }
    *static_cast<GUID*>(propertyData) = PIN_CATEGORY_CAPTURE;
    return S_OK;
}

HRESULT STDMETHODCALLTYPE DjiCameraStream::QuerySupported(REFGUID guidPropSet, DWORD id,
                                                          DWORD* typeSupport) {
    if (guidPropSet != AMPROPSETID_Pin) {
        return E_PROP_SET_UNSUPPORTED;
    }
    if (id != AMPROPERTY_PIN_CATEGORY) {
        return E_PROP_ID_UNSUPPORTED;
    }
    if (typeSupport) {
        *typeSupport = KSPROPERTY_SUPPORT_GET;
    }
    return S_OK;
}

//------------------------------------------------------------------------------
// Registration.
//------------------------------------------------------------------------------

static const AMOVIESETUP_MEDIATYPE kPinType = {&MEDIATYPE_Video, &MEDIASUBTYPE_NV12};

static const AMOVIESETUP_PIN kPin = {
    const_cast<LPWSTR>(L"Capture"), FALSE, TRUE, FALSE, FALSE, &CLSID_NULL, nullptr, 1, &kPinType};

static const AMOVIESETUP_FILTER kFilter = {&CLSID_DjiLiveBridgeCamera, kFilterName, MERIT_DO_NOT_USE,
                                           1, &kPin};

CFactoryTemplate g_Templates[] = {{kFilterName, &CLSID_DjiLiveBridgeCamera,
                                   DjiCameraFilter::CreateInstance, nullptr, &kFilter}};
int g_cTemplates = sizeof(g_Templates) / sizeof(g_Templates[0]);

// Puts the filter in the list apps read when they look for cameras.
static HRESULT RegisterFilter(bool registering) {
    HRESULT result = CoInitialize(nullptr);
    if (FAILED(result)) {
        return result;
    }

    IFilterMapper2* mapper = nullptr;
    result = CoCreateInstance(CLSID_FilterMapper2, nullptr, CLSCTX_INPROC_SERVER,
                              IID_IFilterMapper2, reinterpret_cast<void**>(&mapper));
    if (SUCCEEDED(result)) {
        if (registering) {
            IMoniker* moniker = nullptr;
            REGFILTER2 registration;
            registration.dwVersion = 1;
            registration.dwMerit = MERIT_DO_NOT_USE;
            registration.cPins = 1;
            registration.rgPins = &kPin;
            result = mapper->RegisterFilter(CLSID_DjiLiveBridgeCamera, kFilterName, &moniker,
                                            &CLSID_VideoInputDeviceCategory, nullptr,
                                            &registration);
            if (moniker) {
                moniker->Release();
            }
        } else {
            result = mapper->UnregisterFilter(&CLSID_VideoInputDeviceCategory, nullptr,
                                              CLSID_DjiLiveBridgeCamera);
            // Already gone is not a failure for an uninstaller.
            if (result == HRESULT_FROM_WIN32(ERROR_FILE_NOT_FOUND)) {
                result = S_OK;
            }
        }
        mapper->Release();
    }

    CoUninitialize();
    return result;
}

STDAPI DllRegisterServer() {
    HRESULT result = AMovieDllRegisterServer2(TRUE);
    if (FAILED(result)) {
        return result;
    }
    return RegisterFilter(true);
}

STDAPI DllUnregisterServer() {
    HRESULT result = RegisterFilter(false);
    if (FAILED(result)) {
        return result;
    }
    return AMovieDllRegisterServer2(FALSE);
}

extern "C" BOOL WINAPI DllEntryPoint(HINSTANCE, ULONG, LPVOID);

BOOL APIENTRY DllMain(HANDLE module, DWORD reason, LPVOID reserved) {
    return DllEntryPoint(static_cast<HINSTANCE>(module), reason, reserved);
}
