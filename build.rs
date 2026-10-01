use pipl::*;

const PF_PLUG_IN_VERSION: u16 = 13;
const PF_PLUG_IN_SUBVERS: u16 = 28;

#[rustfmt::skip]
fn main() {
    println!("cargo:rerun-if-changed=src/xbrz_shim.cpp");
    println!("cargo:rerun-if-changed=third_party/xbrz");
    println!("cargo::rustc-check-cfg=cfg(does_dialog, with_premiere, threaded_rendering, catch_panics, uses_audio, sends_update_params_ui, gpu_render, smart_render)");
    // Turn panics into AE errors in release builds too, instead of unwinding into the host.
    println!("cargo:rustc-cfg=catch_panics");

    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .include("third_party/xbrz")
        .file("third_party/xbrz/xbrz.cpp")
        .file("src/xbrz_shim.cpp")
        .define("NDEBUG", None)
        .opt_level(3)
        .compile("xbrz");

    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("xBRZ"),
        Property::Category("Stylize"),

        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),

        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version { major: PF_PLUG_IN_VERSION, minor: PF_PLUG_IN_SUBVERS },
        Property::AE_Effect_Version {
            version: 1,
            subversion: 0,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::DeepColorAware |
            OutFlags::IExpandBuffer
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::SupportsSmartRender |
            OutFlags2::FloatColorAware |
            OutFlags2::SupportsThreadedRendering
        ),
        Property::AE_Effect_Match_Name("NAARI3 xBRZ"),
        Property::AE_Reserved_Info(0),
        Property::AE_Effect_Support_URL("https://github.com/naari3/xbrz-ae"),
    ])
}
