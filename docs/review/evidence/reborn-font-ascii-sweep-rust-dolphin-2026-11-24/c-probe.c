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
    unsigned count = 0;
    for (unsigned size = 7; size <= 40; ++size) {
        if (FT_Set_Pixel_Sizes(face, size, size)) return 2;
        for (unsigned c = 32; c <= 126; ++c) {
            char sample[] = {(char)c, 0};
            SYS_Report("HBC_ASCII size=%u char=%u width=%d\n", size, c, hbc_width(face, sample, has_kerning));
            ++count;
        }
    }
    SYS_Report("HBC_ASCII done count=%u\n", count);
    for (;;) __asm__ volatile("nop");
}
