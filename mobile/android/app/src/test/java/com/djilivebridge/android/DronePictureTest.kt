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
    fun `automatic fills the screen only when little is cut off`() {
        // A vertical drone picture (DJI's vertical mode) on an upright phone: fill, cutting ~20%.
        assertEquals(PictureFit.FILL, shownFit(412f, 915f, 720f / 1280f, PictureFit.AUTO))
        // A wide picture on an upright phone would lose three quarters: show it whole.
        assertEquals(PictureFit.WHOLE, shownFit(412f, 915f, wide, PictureFit.AUTO))
        // The same wide picture with the phone turned sideways: fill.
        assertEquals(PictureFit.FILL, shownFit(915f, 412f, wide, PictureFit.AUTO))
        // A choice the user made is kept whatever the shape.
        assertEquals(PictureFit.WHOLE, shownFit(915f, 412f, wide, PictureFit.WHOLE))
        assertEquals(PictureFit.FILL, shownFit(412f, 915f, wide, PictureFit.FILL))
        assertSize(1564.4f to 880f, pictureSize(400f, 880f, wide, PictureFit.FILL))
        assertSize(400f to 225f, pictureSize(400f, 880f, wide, PictureFit.AUTO))
    }

    @Test
    fun `the choice is stored by name and starts automatic`() {
        assertEquals(PictureFit.FILL, PictureFit.fromStorage("fill"))
        assertEquals(PictureFit.WHOLE, PictureFit.fromStorage("whole"))
        assertEquals(PictureFit.AUTO, PictureFit.fromStorage("auto"))
        assertEquals(PictureFit.AUTO, PictureFit.fromStorage(null))
        assertEquals(PictureFit.AUTO, PictureFit.fromStorage("something else"))
    }
}
