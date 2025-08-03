#!/usr/bin/env bash




set -e  


APP_NAME="KeyboardCleanBlock"
BUNDLE_ID="io.yesolutions.keyboardcleanblock"  
EXECUTABLE_NAME="kbcleanblock"
VERSION="1.0.0"
BUILD_NUMBER="1"



RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' 

echo -e "${BLUE}🚀 Building ${APP_NAME} v${VERSION}${NC}"
echo "=================================="


command -v cargo >/dev/null 2>&1 || { echo -e "${RED}❌ Rust/Cargo is required but not installed.${NC}" >&2; exit 1; }
command -v codesign >/dev/null 2>&1 || { echo -e "${RED}❌ codesign is required but not found.${NC}" >&2; exit 1; }
command -v xcrun >/dev/null 2>&1 || { echo -e "${RED}❌ Xcode command line tools are required.${NC}" >&2; exit 1; }


echo -e "${YELLOW}🧹 Cleaning previous builds...${NC}"
rm -rf "${APP_NAME}.app"
rm -f "${APP_NAME}.dmg"
#cargo clean


echo -e "${YELLOW}🔨 Building Rust executable...${NC}"
cargo build --release
if [ ! -f "target/release/${EXECUTABLE_NAME}" ]; then
    echo -e "${RED}❌ Build failed - executable not found${NC}"
    exit 1
fi


echo -e "${YELLOW}📁 Creating app bundle structure...${NC}"
mkdir -p "${APP_NAME}.app/Contents/MacOS"
mkdir -p "${APP_NAME}.app/Contents/Resources"


cp "target/release/${EXECUTABLE_NAME}" "${APP_NAME}.app/Contents/MacOS/"


cp "Info.plist" "${APP_NAME}.app/Contents/"


if [ -f "icon.png" ]; then
    echo -e "${YELLOW}🎨 Creating app icon...${NC}"

    
    cp "icon.png" "${APP_NAME}.app/Contents/Resources/"
    echo -e "${GREEN}✅ Copied icon.png to Resources folder${NC}"

    
    mkdir -p "${APP_NAME}.app/Contents/Resources/AppIcon.iconset"

    
    
    sips -z 16 16 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_16x16.png" >/dev/null 2>&1
    sips -z 32 32 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_16x16@2x.png" >/dev/null 2>&1
    sips -z 32 32 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_32x32.png" >/dev/null 2>&1
    sips -z 64 64 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_32x32@2x.png" >/dev/null 2>&1
    sips -z 128 128 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_128x128.png" >/dev/null 2>&1
    sips -z 256 256 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_128x128@2x.png" >/dev/null 2>&1
    sips -z 256 256 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_256x256.png" >/dev/null 2>&1
    sips -z 512 512 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_256x256@2x.png" >/dev/null 2>&1
    sips -z 512 512 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_512x512.png" >/dev/null 2>&1
    sips -z 1024 1024 icon.png --out "${APP_NAME}.app/Contents/Resources/AppIcon.iconset/icon_512x512@2x.png" >/dev/null 2>&1

    
    iconutil -c icns "${APP_NAME}.app/Contents/Resources/AppIcon.iconset" -o "${APP_NAME}.app/Contents/Resources/AppIcon.icns"

    
    rm -rf "${APP_NAME}.app/Contents/Resources/AppIcon.iconset"

    
    if [ -f "${APP_NAME}.app/Contents/Resources/AppIcon.icns" ]; then
        echo -e "${GREEN}✅ App icon created: AppIcon.icns${NC}"
        ls -la "${APP_NAME}.app/Contents/Resources/AppIcon.icns"
    else
        echo -e "${RED}❌ Failed to create app icon${NC}"
    fi

else
    echo -e "${YELLOW}⚠️  No icon.png found - app will use default icon${NC}"
    echo -e "${BLUE}💡 To add an icon, create a 1024x1024 PNG file named 'icon.png'${NC}"
fi


chmod +x "${APP_NAME}.app/Contents/MacOS/${EXECUTABLE_NAME}"


echo -e "${YELLOW}🧹 Cleaning metadata for code signing...${NC}"
find "${APP_NAME}.app" -name '.DS_Store' -delete
find "${APP_NAME}.app" -name '__MACOSX' -exec rm -rf {} + 2>/dev/null || true
find "${APP_NAME}.app" -name '.AppleDouble' -exec rm -rf {} + 2>/dev/null || true
find "${APP_NAME}.app" -name '.LSOverride' -delete 2>/dev/null || true
find "${APP_NAME}.app" -name 'Icon?' -delete 2>/dev/null || true


xattr -cr "${APP_NAME}.app" 2>/dev/null || true


echo -e "${YELLOW}🔍 Verifying app bundle structure...${NC}"
ls -la "${APP_NAME}.app/Contents/MacOS/"


echo -e "${YELLOW}🔐 Signing application...${NC}"


codesign --force --options runtime --sign "${DEVELOPER_ID}" "${APP_NAME}.app/Contents/MacOS/${EXECUTABLE_NAME}"


codesign --force --options runtime --sign "${DEVELOPER_ID}" "${APP_NAME}.app"


echo -e "${YELLOW}🔍 Verifying signature...${NC}"
codesign --verify --verbose "${APP_NAME}.app"
echo -e "${BLUE}💡 Note: 'rejected' status is expected for unnotarized apps${NC}"
spctl --assess --verbose "${APP_NAME}.app" || echo -e "${YELLOW}⚠️  App will be notarized in next steps${NC}"


echo -e "${YELLOW}📦 Creating installer DMG...${NC}"


DMG_FOLDER="${APP_NAME} ${VERSION}"
rm -rf "${DMG_FOLDER}"
mkdir "${DMG_FOLDER}"


cp -R "${APP_NAME}.app" "${DMG_FOLDER}/"


ln -sf /Applications "${DMG_FOLDER}/Applications"


DMG_NAME="${APP_NAME}-${VERSION}"
hdiutil create -volname "${DMG_FOLDER}" -srcfolder "${DMG_FOLDER}" -ov -format UDZO "${DMG_NAME}.dmg"


rm -rf "${DMG_FOLDER}"

if [ -f "${DMG_NAME}.dmg" ]; then
    echo -e "${GREEN}✅ Installer DMG created: ${DMG_NAME}.dmg${NC}"
    ls -la "${DMG_NAME}.dmg"
else
    echo -e "${RED}❌ DMG creation failed${NC}"
    exit 1
fi


echo -e "${YELLOW}🔐 Signing DMG...${NC}"
if codesign --force --sign "${DEVELOPER_ID}" "${DMG_NAME}.dmg"; then
    echo -e "${GREEN}✅ DMG signed successfully${NC}"
else
    echo -e "${RED}❌ DMG signing failed${NC}"
    exit 1
fi


if [ ! -f "${DMG_NAME}.dmg" ]; then
    echo -e "${RED}❌ DMG file not found!${NC}"
    exit 1
fi

echo -e "${GREEN}✅ DMG created and signed: ${DMG_NAME}.dmg${NC}"
ls -la "${DMG_NAME}.dmg"


echo -e "${YELLOW}📋 Starting notarization...${NC}"
echo -e "${BLUE}💡 Note: Notarization requires a valid Apple Developer account and app-specific password${NC}"
echo -e "${BLUE}   You can skip this step for testing, but it's required for distribution${NC}"

read -p "Do you want to notarize the DMG? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${YELLOW}📤 Uploading for notarization...${NC}"

    
    

    xcrun notarytool submit "${DMG_NAME}.dmg" --keychain-profile "AC_PASSWORD" --wait

    if [ $? -eq 0 ]; then
        echo -e "${YELLOW}📎 Stapling notarization ticket...${NC}"
        xcrun stapler staple "${DMG_NAME}.dmg"
        echo -e "${GREEN}✅ Notarization complete${NC}"
    else
        echo -e "${RED}❌ Notarization failed${NC}"
    fi
else
    echo -e "${YELLOW}⏭️  Skipping notarization${NC}"
fi


echo -e "${YELLOW}🔢 Generating checksums...${NC}"
shasum -a 256 "${DMG_NAME}.dmg" > "${DMG_NAME}.dmg.sha256"


echo -e "${YELLOW}🔍 Final verification...${NC}"
codesign --verify --verbose "${DMG_NAME}.dmg"


echo
echo -e "${GREEN}🎉 Build Complete!${NC}"
echo "=================================="
echo -e "${BLUE}📱 App Bundle:${NC} ${APP_NAME}.app"
echo -e "${BLUE}💿 Installer DMG:${NC} ${DMG_NAME}.dmg"
echo -e "${BLUE}🔢 Checksum:${NC} ${DMG_NAME}.dmg.sha256"
echo
echo -e "${BLUE}📋 Next Steps:${NC}"
echo "1. Test the app bundle locally"
echo "2. Distribute the DMG file to users"
echo "3. Users will need to grant accessibility permissions on first run"
echo
echo -e "${YELLOW}⚠️  Important:${NC} Remember to update the bundle ID and developer credentials in this script!"
