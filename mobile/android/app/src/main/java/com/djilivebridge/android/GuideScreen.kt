package com.djilivebridge.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.PhoneAndroid
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

private const val GUIDE_PAGE_COUNT = 3

/** First-run walkthrough; the help button in the top bar opens it again at any time. */
@Composable
internal fun GuideScreen(onFinish: () -> Unit) {
    val colors = BridgeTheme.colors
    val pagerState = rememberPagerState(pageCount = { GUIDE_PAGE_COUNT })
    val scope = rememberCoroutineScope()
    val lastPage = pagerState.currentPage == GUIDE_PAGE_COUNT - 1

    BackHandler {
        if (pagerState.currentPage > 0) {
            scope.launch { pagerState.animateScrollToPage(pagerState.currentPage - 1) }
        } else {
            onFinish()
        }
    }

    Surface(color = colors.background, contentColor = colors.text) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .windowInsetsPadding(WindowInsets.safeDrawing),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(min = 56.dp)
                    .padding(horizontal = 8.dp),
                horizontalArrangement = Arrangement.End,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (!lastPage) TextButton(onClick = onFinish) { Text("Geç") }
            }
            HorizontalPager(
                state = pagerState,
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
            ) { page ->
                GuidePage(page)
            }
            Column(
                modifier = Modifier
                    .widthIn(max = 520.dp)
                    .fillMaxWidth()
                    .padding(horizontal = 24.dp, vertical = 16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(24.dp),
            ) {
                PageIndicator(count = GUIDE_PAGE_COUNT, current = pagerState.currentPage)
                PrimaryButton(
                    text = if (lastPage) "Başla" else "Devam",
                    onClick = {
                        if (lastPage) {
                            onFinish()
                        } else {
                            scope.launch { pagerState.animateScrollToPage(pagerState.currentPage + 1) }
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun GuidePage(page: Int) {
    BoxWithConstraints(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        // Centered when it fits, scrollable when the font is large or the screen is short.
        Column(
            modifier = Modifier
                .widthIn(max = 520.dp)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .heightIn(min = maxHeight)
                .padding(horizontal = 28.dp, vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(28.dp, Alignment.CenterVertically),
        ) {
            when (page) {
                0 -> {
                    FlowIllustration()
                    PageText(
                        title = "Drone'dan canlı yayına",
                        body = "Kumandadaki DJI Fly görüntüyü bu telefona gönderir; telefon da seçtiğin " +
                            "platforma iletir.",
                    )
                }
                1 -> {
                    AddressIllustration()
                    PageText(
                        title = "Önce drone'u bağla",
                        body = "Ekrandaki adresi DJI Fly'da RTMP alanına yaz. Drone'un görüntüsü hemen " +
                            "telefonda görünür; henüz hiçbir yerde yayında değilsin.",
                    )
                }
                else -> {
                    PlatformsIllustration()
                    PageText(
                        title = "Sonra yayına geç",
                        body = "Platformunu seç, yayın anahtarını yapıştır ve Canlı yayını başlat'a bas. " +
                            "Sunucu adresleri hazır.",
                    )
                }
            }
        }
    }
}

@Composable
private fun PageText(title: String, body: String) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(
            modifier = Modifier.semantics { heading() },
            text = title,
            style = MaterialTheme.typography.headlineSmall,
            textAlign = TextAlign.Center,
        )
        Text(
            text = body,
            style = MaterialTheme.typography.bodyLarge,
            color = BridgeTheme.colors.muted,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun FlowIllustration() {
    Row(
        modifier = Modifier.clearAndSetSemantics {},
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        IllustrationCircle(painterResource(R.drawable.ic_drone))
        FlowArrow()
        IllustrationCircle(rememberVectorPainter(Icons.Rounded.PhoneAndroid))
        FlowArrow()
        Row(horizontalArrangement = Arrangement.spacedBy((-10).dp)) {
            listOf(DestinationKind.INSTAGRAM, DestinationKind.TIKTOK, DestinationKind.YOUTUBE).forEach { kind ->
                PlatformTile(
                    kind = kind,
                    size = 44.dp,
                    modifier = Modifier.border(2.dp, BridgeTheme.colors.background, RoundedCornerShape(12.dp)),
                )
            }
        }
    }
}

@Composable
private fun IllustrationCircle(icon: Painter) {
    val colors = BridgeTheme.colors
    Box(
        modifier = Modifier
            .size(72.dp)
            .background(colors.accentSoft, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = colors.link, modifier = Modifier.size(32.dp))
    }
}

@Composable
private fun FlowArrow() {
    Icon(
        imageVector = Icons.Rounded.ChevronRight,
        contentDescription = null,
        tint = BridgeTheme.colors.faint,
        modifier = Modifier.size(22.dp),
    )
}

@Composable
private fun PlatformsIllustration() {
    Column(
        modifier = Modifier.clearAndSetSemantics {},
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        DestinationKind.entries.chunked(4).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                row.forEach { kind -> PlatformTile(kind, 56.dp) }
            }
        }
    }
}

@Composable
private fun AddressIllustration() {
    val colors = BridgeTheme.colors
    Column(
        modifier = Modifier.clearAndSetSemantics {},
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        IllustrationCircle(painterResource(R.drawable.ic_drone))
        Surface(shape = MaterialTheme.shapes.small, color = colors.card, contentColor = colors.text) {
            Text(
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
                text = "rtmp://192.168.1.20:1935/drone",
                fontFamily = FontFamily.Monospace,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}

@Composable
private fun PageIndicator(count: Int, current: Int) {
    val colors = BridgeTheme.colors
    Row(
        modifier = Modifier.clearAndSetSemantics { contentDescription = "Sayfa ${current + 1} / $count" },
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        repeat(count) { index ->
            val selected = index == current
            val width by animateDpAsState(if (selected) 22.dp else 8.dp, label = "pageIndicatorWidth")
            Box(
                modifier = Modifier
                    .height(8.dp)
                    .width(width)
                    .clip(CircleShape)
                    .background(if (selected) colors.accent else colors.fieldStrong),
            )
        }
    }
}
