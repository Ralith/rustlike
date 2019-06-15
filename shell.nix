with import <nixpkgs> { };
let
vulkan-loader-dbg = vulkan-loader.overrideAttrs (_: { dontStrip = true; cmakeBuildType = "Debug"; });
dlopen-libs = with xlibs; [ vulkan-loader-dbg vulkan-validation-layers libX11 libXcursor libXrandr libXi libglvnd ];
in stdenv.mkDerivation {
  name = "rustlike";
  nativeBuildInputs = with pkgs; [ rustChannels.stable.rust cmake python3 ];
  SHADERC_LIB_DIR = "${shaderc.static}/lib";
  shellHook = ''
    export RUST_BACKTRACE=1
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${stdenv.lib.makeLibraryPath dlopen-libs}"
    export VK_INSTANCE_LAYERS=VK_LAYER_LUNARG_standard_validation
    export XDG_DATA_DIRS="$XDG_DATA_DIRS:${vulkan-validation-layers}/share"
  '';
}
