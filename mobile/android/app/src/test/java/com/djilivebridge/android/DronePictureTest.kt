package com.djilivebridge.android

import org.junit.Assert.assertEquals
import org.junit.Test

class DronePictureTest {
    private val wide = 16f / 9f

    private fun assertSize(expected: Pair<Float, Float>, actual: Pair<Float, Float>) {
        assertEquals(expected.first, actual.first, 0.5f)
        assertEquals(expected.second, actual.second, 0.5f)
    }

    @Test
    fun `a wide picture on a phone held upright`() {
        // The whole picture spans the width; filling the screen cuts off its sides.
        assertSize(400f to 225f, pictureSize(400f, 880f, wide, PictureFit.WHOLE))
        assertSize(1564.4f to 880f, pictureSize(400f, 880f, wide, PictureFit.FILL))
    }

    @Test
    fun `a wide picture on a phone turned sideways`() {
        // A 20:9 screen is wider than 16:9: bars at the sides, or a slight cut at the top and bottom.
        assertSize(711.1f to 400f, pictureSize(880f, 400f, wide, PictureFit.WHOLE))
        assertSize(880f to 495f, pictureSize(880f, 400f, wide, PictureFit.FILL))
    }

    @Test
    fun `a picture of the screen's own shape fills it either way`() {
        assertSize(1600f to 900f, pictureSize(1600f, 900f, wide, PictureFit.WHOLE))
        assertSize(1600f to 900f, pictureSize(1600f, 900f, wide, PictureFit.FILL))
    }

    @Test
    fun `the choice is stored by name and defaults to the whole picture`() {
        assertEquals(PictureFit.FILL, PictureFit.fromStorage("fill"))
        assertEquals(PictureFit.WHOLE, PictureFit.fromStorage("whole"))
        assertEquals(PictureFit.WHOLE, PictureFit.fromStorage(null))
        assertEquals(PictureFit.WHOLE, PictureFit.fromStorage("something else"))
    }
}
