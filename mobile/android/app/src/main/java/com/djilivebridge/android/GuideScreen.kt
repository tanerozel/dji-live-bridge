package com.djilivebridge.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowForward
import androidx.compose.material.icons.automirrored.rounded.HelpOutline
import androidx.compose.material.icons.rounded.Checklist
import androidx.compose.material.icons.rounded.Key
import androidx.compose.material.icons.rounded.LiveTv
import androidx.compose.material.icons.rounded.PhoneAndroid
import androidx.compose.material.icons.rounded.RocketLaunch
import androidx.compose.material.icons.rounded.Wifi
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
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

private const val GUIDE_PAGE_COUNT = 3

/** First-run walkthrough; the help button in the top bar opens it again at any time. */
@Composable
internal fun GuideScreen(onFinish: () -> Unit) {
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

    Surface(color = MaterialTheme.colorScheme.background) {
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
                    .padding(start = 20.dp, end = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                BrandMark(size = 28.dp)
                Text(
                    modifier = Modifier.weight(1f),
                    text = "DJI Live Bridge",
                    style = MaterialTheme.typography.titleSmall,
                )
                if (!lastPage) {
                    TextButton(onClick = onFinish) { Text("Geç") }
                }
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
                    .padding(horizontal = 20.dp, vertical = 16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                PageIndicator(count = GUIDE_PAGE_COUNT, current = pagerState.currentPage)
                PrimaryActionButton(
                    text = if (lastPage) "Başlayalım" else "Devam",
                    onClick = {
                        if (lastPage) {
                            onFinish()
                        } else {
                            scope.launch { pagerState.animateScrollToPage(pagerState.currentPage + 1) }
                        }
                    },
                    trailingIcon = if (lastPage) null else Icons.AutoMirrored.Rounded.ArrowForward,
                )
            }
        }
    }
}

@Composable
private fun GuidePage(page: Int) {
    BoxWithConstraints(
        modifier = Modifier.fillMaxSize(),
        contentAlignment = Alignment.TopCenter,
    ) {
        // Centered when it fits, scrollable when the font is large or the screen is short.
        Column(
            modifier = Modifier
                .widthIn(max = 520.dp)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .heightIn(min = maxHeight)
                .padding(horizontal = 20.dp, vertical = 12.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(20.dp, Alignment.CenterVertically),
        ) {
            when (page) {
                0 -> HowItWorksPage()
                1 -> RequirementsPage()
                else -> StepsPage()
            }
        }
    }
}

@Composable
private fun HowItWorksPage() {
    BridgeCard(contentPadding = PaddingValues(vertical = 28.dp, horizontal = 12.dp)) {
        PipelineView(
            source = PipelineNode("Kumanda", "DJI RC 2", NodeTone.BUSY, painterResource(R.drawable.ic_drone)),
            relay = PipelineNode(
                "Bu telefon",
                "Köprü",
                NodeTone.BUSY,
                rememberVectorPainter(Icons.Rounded.PhoneAndroid),
            ),
            target = PipelineNode(
                "Platform",
                "TikTok · YouTube",
                NodeTone.BUSY,
                rememberVectorPainter(Icons.Rounded.LiveTv),
            ),
            sourceLinkActive = true,
            targetLinkActive = true,
            activeLinkColor = MaterialTheme.colorScheme.primary,
        )
    }
    PageText(
        title = "Drone görüntün, telefonundan canlı yayında",
        body = "DJI RC 2 kumandan yayını bu telefona gönderir. Telefon da görüntüyü hiç " +
            "değiştirmeden TikTok, YouTube ya da kendi RTMP sunucuna iletir.",
    )
    TipBox(
        text = "Bilgisayar ya da ek uygulama gerekmez; kumanda ile telefon yeterli.",
        icon = Icons.Rounded.RocketLaunch,
    )
}

@Composable
private fun RequirementsPage() {
    GradientIcon(Icons.Rounded.Checklist)
    PageText(
        title = "Başlamadan önce",
        body = "Bunları bir kez hazırlarsın; sonraki yayınlarda doğrudan başlarsın.",
    )
    BridgeCard {
        RequirementRow(
            icon = rememberVectorPainter(Icons.Rounded.Key),
            title = "Yayın anahtarı",
            text = "Platformunun canlı yayın ayarlarından sunucu adresini ve yayın anahtarını al.",
        )
        RequirementRow(
            icon = rememberVectorPainter(Icons.Rounded.Wifi),
            title = "Aynı ağ",
            text = "Kumanda ve telefon aynı Wi-Fi'da olsun ya da kumandayı telefonun hotspot'una bağla.",
        )
        RequirementRow(
            icon = painterResource(R.drawable.ic_drone),
            title = "DJI Fly",
            text = "Kumandadaki DJI Fly'ın RTMP ile canlı yayın özelliğini kullanacaksın.",
        )
    }
}

@Composable
private fun StepsPage() {
    GradientIcon(Icons.Rounded.RocketLaunch)
    PageText(
        title = "Dört adımda yayındasın",
        body = "Ana ekran bu adımları sırayla gösterir; sıradaki adım hep mavi çerçevelidir.",
    )
    BridgeCard {
        NumberedItem(
            number = 1,
            title = "Yayın hedefini ekle",
            text = "Nereye yayın yapacağını seç; sunucu adresini ve anahtarını bir kez kaydet.",
        )
        NumberedItem(
            number = 2,
            title = "Aynı ağa bağlan",
            text = "Telefonu Wi-Fi'a bağla ya da hotspot'u aç; kumandayı da aynı ağa bağla.",
        )
        NumberedItem(
            number = 3,
            title = "Köprüyü başlat",
            text = "Telefon, kumandadan gelecek görüntüyü beklemeye başlar.",
        )
        NumberedItem(
            number = 4,
            title = "DJI Fly'da yayını aç",
            text = "Uygulamanın gösterdiği adresi DJI Fly'a yaz ve yayını başlat.",
        )
    }
    InfoNote(
        text = "Takılırsan sağ üstteki ? simgesiyle bu rehberi yeniden açabilirsin.",
        icon = Icons.AutoMirrored.Rounded.HelpOutline,
    )
}

@Composable
private fun PageText(title: String, body: String) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
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
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun GradientIcon(icon: ImageVector) {
    val colors = BridgeTheme.colors
    Box(
        modifier = Modifier
            .padding(top = 12.dp)
            .size(88.dp)
            .clip(CircleShape)
            .background(Brush.linearGradient(listOf(colors.brandStart, colors.brandEnd))),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, contentDescription = null, tint = Color.White, modifier = Modifier.size(44.dp))
    }
}

@Composable
private fun RequirementRow(icon: Painter, title: String, text: String) {
    val scheme = MaterialTheme.colorScheme
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Surface(
            modifier = Modifier.size(44.dp),
            shape = CircleShape,
            color = scheme.primaryContainer,
        ) {
            Box(contentAlignment = Alignment.Center) {
                Icon(icon, contentDescription = null, tint = scheme.primary, modifier = Modifier.size(22.dp))
            }
        }
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(text = title, style = MaterialTheme.typography.titleSmall)
            Text(
                text = text,
                style = MaterialTheme.typography.bodyMedium,
                color = scheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun PageIndicator(count: Int, current: Int) {
    Row(
        modifier = Modifier.clearAndSetSemantics {
            contentDescription = "Sayfa ${current + 1} / $count"
        },
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        repeat(count) { index ->
            val selected = index == current
            val width by animateDpAsState(if (selected) 24.dp else 8.dp, label = "pageIndicatorWidth")
            Box(
                modifier = Modifier
                    .height(8.dp)
                    .width(width)
                    .clip(CircleShape)
                    .background(
                        if (selected) {
                            MaterialTheme.colorScheme.primary
                        } else {
                            MaterialTheme.colorScheme.outlineVariant
                        },
                    ),
            )
        }
    }
}
