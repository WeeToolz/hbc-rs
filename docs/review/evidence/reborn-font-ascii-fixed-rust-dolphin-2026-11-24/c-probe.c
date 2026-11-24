#include <ogc/system.h>
#include <ft2build.h>
#include FT_FREETYPE_H

extern const unsigned char _binary_assets_droid_ttf_start[];
extern const unsigned char _binary_assets_droid_ttf_end[];

static int hbc_width(FT_Face face, const char *text, int has_kerning) {
    FT_Pos cdx = 0, cdy = 0;
    FT_UInt previous = 0;
    int cx = 0;
    while (*text) {
        unsigned char character = (unsigned char)*text++;
        if (character == '\n') {
            if (*text) character = ' ';
            else continue;
        }
        if (character == '\r') continue;
        FT_UInt glyph_index = FT_Get_Char_Index(face, character);
        if (previous && has_kerning) {
            FT_Vector delta;
            if (!FT_Get_Kerning(face, previous, glyph_index, FT_KERNING_UNFITTED, &delta)) {
                cdx += delta.x;
                cdy += delta.y;
            }
        }
        cx += (cdx + 32) >> 6;
        if (FT_Load_Glyph(face, glyph_index, FT_LOAD_DEFAULT) ||
            FT_Render_Glyph(face->glyph, FT_RENDER_MODE_NORMAL))
            return -1;
        cdx = face->glyph->advance.x;
        cdy = face->glyph->advance.y;
        previous = glyph_index;
    }
    (void)cdy;
    cx += (cdx + 32) >> 6;
    return cx;
}

int main(void) {
    FT_Library library;
    FT_Face face;
    const FT_Long length = (FT_Long)(_binary_assets_droid_ttf_end - _binary_assets_droid_ttf_start);
    if (FT_Init_FreeType(&library) ||
        FT_New_Memory_Face(library, _binary_assets_droid_ttf_start, length, 0, &face)) {
        SYS_Report("HBC_FONT failed to initialize FreeType\n");
        return 1;
    }
    const int has_kerning = FT_HAS_KERNING(face);
    SYS_Report("HBC_FONT has_kerning=%u\n", (unsigned)has_kerning);
    const unsigned sizes[] = {16, 24, 24, 24, 24};
    const char chars[] = {'a', 'N', 'v', '{', '}'};
    for (unsigned i = 0; i < sizeof(chars); ++i) {
        FT_Set_Pixel_Sizes(face, sizes[i], sizes[i]);
        FT_UInt glyph = FT_Get_Char_Index(face, chars[i]);
        if (FT_Load_Glyph(face, glyph, FT_LOAD_DEFAULT)) return 2;
        long hinted = face->glyph->advance.x;
        if (FT_Load_Glyph(face, glyph, FT_LOAD_NO_HINTING)) return 3;
        long unhinted = face->glyph->advance.x;
        SYS_Report("HBC_HINT size=%u char=%u hinted=%ld unhinted=%ld\n", sizes[i], (unsigned char)chars[i], hinted, unhinted);
    }
    SYS_Report("HBC_HINT done count=5\n");
    for (;;) __asm__ volatile("nop");
}
