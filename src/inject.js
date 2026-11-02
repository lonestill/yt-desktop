(function () {
    'use strict';

    function postIpc(obj) {
        try {
            const raw = JSON.stringify(obj);
            if (window.ipc && typeof window.ipc.postMessage === 'function') {
                window.ipc.postMessage(raw);
                return true;
            }
            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ipc) {
                window.webkit.messageHandlers.ipc.postMessage(raw);
                return true;
            }
        } catch (_) {}
        return false;
    }

    // Trusted Types bypass policy (if supported)
    let trustedPolicy = null;
    if (window.trustedTypes && typeof window.trustedTypes.createPolicy === 'function') {
        try {
            trustedPolicy = window.trustedTypes.createPolicy('default', {
                createHTML: (s) => s,
            });
        } catch (_) {
            try {
                trustedPolicy = window.trustedTypes.createPolicy('yt-desktop-policy', {
                    createHTML: (s) => s,
                });
            } catch (_) {}
        }
    }
    const _log = console.log;
    console.log = function (...args) {
        _log.apply(console, args);
        postIpc({
            type: 'log',
            level: 'INFO',
            message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ')
        });
    };

    const _error = console.error;
    console.error = function (...args) {
        _error.apply(console, args);
        postIpc({
            type: 'log',
            level: 'ERROR',
            message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ')
        });
    };

    window.addEventListener('error', function (e) {
        // Suppress generic cross-origin "Script error." and YouTube's internal kevlar errors
        if (e.message === 'Script error.' && (!e.filename || e.filename === '')) {
            return;
        }
        if (e.filename && e.filename.includes('kevlar')) {
            return;
        }
        postIpc({
            type: 'log',
            level: 'UNCAUGHT',
            message: (e.message || String(e)) + ' at ' + (e.filename || '') + ':' + (e.lineno || 0)
        });
    });

    let userConfig = {
        block_ads: true,
        sponsor_block: true,
        return_dislikes: true,
        block_shorts: true,
        auto_confirm_watching: true,
        enable_pip: true,
        oled_black: false,
        max_quality: true
    };

    window.__applyConfig = function(cfg) {
        userConfig = Object.assign(userConfig, cfg);
        if (userConfig.block_shorts) {
            document.documentElement.classList.add('yt-hide-shorts');
        } else {
            document.documentElement.classList.remove('yt-hide-shorts');
        }
        if (userConfig.oled_black) {
            document.documentElement.classList.add('yt-oled-black');
        } else {
            document.documentElement.classList.remove('yt-oled-black');
        }
    };

    // 1. Inject Styles
    function injectStyles() {
        if (document.getElementById('yt-desktop-styles')) return;
        const head = document.head || document.documentElement;
        if (!head) return;

        const style = document.createElement('style');
        style.id = 'yt-desktop-styles';
        style.textContent = window.__YT_DESKTOP_CSS__ || '';
        head.appendChild(style);

        if (userConfig.block_shorts) {
            document.documentElement.classList.add('yt-hide-shorts');
        }
        if (userConfig.oled_black) {
            document.documentElement.classList.add('yt-oled-black');
        }
    }

    function formatCount(num) {
        if (num === null || num === undefined) return '';
        if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
        if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
        return String(num);
    }

    function getActiveVideoId() {
        // 1. URL parameter (?v=...)
        try {
            const urlParams = new URLSearchParams(window.location.search);
            const v = urlParams.get('v');
            if (v && v.length === 11) return v;
        } catch (_) {}

        // 2. Pathname pattern (/shorts/..., /live/..., /watch/...)
        try {
            const match = window.location.pathname.match(/\/(shorts|live|watch)\/([a-zA-Z0-9_-]{11})/);
            if (match && match[2]) return match[2];
        } catch (_) {}

        // 3. Flexy attribute
        try {
            const flexy = document.querySelector('ytd-watch-flexy');
            if (flexy) {
                const vid = flexy.getAttribute('video-id');
                if (vid && vid.length === 11) return vid;
            }
        } catch (_) {}

        // 4. Player API
        try {
            const player = document.getElementById('movie_player');
            if (player && typeof player.getVideoData === 'function') {
                const data = player.getVideoData();
                if (data && data.video_id && data.video_id.length === 11) return data.video_id;
            }
        } catch (_) {}

        return null;
    }

    // 2. Shorts Auto-Redirect
    function checkShortsRedirect() {
        if (!userConfig.block_shorts) return;
        const path = window.location.pathname;
        if (path.startsWith('/shorts/')) {
            const videoId = path.split('/')[2];
            if (videoId) {
                window.location.replace(`/watch?v=${videoId}`);
            }
        }
    }

    // 3. Video Ad Skipper & Fast-Forward Engine
    let wasAdShowing = false;
    let savedPlaybackRate = 1.0;

    function handleVideoAds() {
        if (!userConfig.block_ads) return;

        const video = document.querySelector('video');
        const player = document.getElementById('movie_player') || document.querySelector('.html5-video-player');

        const adModule = document.querySelector('.ytp-ad-module');
        const hasAdOverlay = !!document.querySelector('.ytp-ad-player-overlay') || !!document.querySelector('.ad-showing') || !!document.querySelector('.ad-interrupting');
        const hasAdModuleChildren = adModule && adModule.children.length > 0;
        const playerHasAdClass = player && (player.classList.contains('ad-showing') || player.classList.contains('ad-interrupting'));

        const isAdActive = hasAdOverlay || hasAdModuleChildren || playerHasAdClass;

        if (isAdActive && video) {
            if (!wasAdShowing) {
                wasAdShowing = true;
                savedPlaybackRate = video.playbackRate > 2.0 ? 1.0 : video.playbackRate;
            }

            video.muted = true;
            video.playbackRate = 16.0;

            if (isFinite(video.duration) && video.duration > 0) {
                video.currentTime = video.duration;
            }

            const skipSelectors = [
                '.ytp-ad-skip-button',
                '.ytp-skip-ad-button',
                '.ytp-ad-skip-button-modern',
                '.ytp-ad-skip-button-text',
                'button.ytp-ad-skip-button-icon',
                '.ytp-ad-overlay-close-button'
            ];

            for (const sel of skipSelectors) {
                const btn = document.querySelector(sel);
                if (btn) {
                    btn.click();
                    break;
                }
            }
        } else if (wasAdShowing && video) {
            wasAdShowing = false;
            video.muted = false;
            video.playbackRate = savedPlaybackRate;
        }
    }

    // 4. Anti-Adblock Modal Remover
    function handleAntiAdblock() {
        if (!userConfig.block_ads) return;

        const dialog = document.querySelector('ytd-enforcement-message-view-model') ||
            document.querySelector('tp-yt-paper-dialog:has(#feedback)');

        if (dialog) {
            const paperDialog = dialog.closest('tp-yt-paper-dialog') || dialog;
            paperDialog.remove();

            document.querySelectorAll('tp-yt-iron-overlay-backdrop').forEach(el => el.remove());
            document.body.style.overflow = 'auto';
            document.documentElement.style.overflow = 'auto';

            const video = document.querySelector('video');
            if (video && video.paused) {
                video.play().catch(() => {});
            }
        }
    }

    // 5. Metadata Sync with Native Rust Backend
    let requestedVideoId = null;
    let lastRequestTime = 0;
    let videoMetaCache = {};
    let lastSkippedUuid = null;

    function syncVideoMeta() {
        const videoId = getActiveVideoId();
        if (!videoId) return;

        const cached = videoMetaCache[videoId];
        const isPendingTooLong = (Date.now() - lastRequestTime) > 7000;

        if (!cached && (videoId !== requestedVideoId || isPendingTooLong)) {
            requestedVideoId = videoId;
            lastRequestTime = Date.now();
            console.log('[EXT-SYNC] 🔍 Requesting metadata for videoId: ' + videoId);
            postIpc({
                type: 'fetch_video_meta',
                video_id: videoId
            });
        }
    }

    window.__onVideoMeta = function(videoId, dislikesData, sponsorSegments) {
        try {
            console.log('[EXT-SYNC] 📥 Received meta for ' + videoId + ': dislikes=' + (dislikesData ? dislikesData.dislikes : 'none') + ', segments=' + (sponsorSegments ? sponsorSegments.length : 0));
            videoMetaCache[videoId] = {
                dislikes: dislikesData,
                segments: sponsorSegments || []
            };
            applyDislikes();
            renderSponsorSegments();
        } catch (err) {
            console.error('[EXT-SYNC] ❌ Error in __onVideoMeta: ' + err.name + ': ' + err.message + ' | ' + err.stack);
        }
    };

    // 6. Return YouTube Dislike DOM Injection
    function getDislikeButtonHost() {
        const segViewModel = document.querySelector('segmented-like-dislike-button-view-model');
        if (segViewModel) {
            const btn = segViewModel.querySelector('dislike-button-view-model') || segViewModel.children[1];
            if (btn) return btn;
        }
        const legacySeg = document.querySelector('ytd-segmented-like-dislike-button-renderer');
        if (legacySeg) {
            const btn = legacySeg.querySelector('#segmented-dislike-button') || legacySeg.children[1];
            if (btn) return btn;
        }
        const candidates = document.querySelectorAll(
            'dislike-button-view-model, ' +
            '#segmented-dislike-button, ' +
            'button[aria-label*="не нравится" i], ' +
            'button[aria-label*="dislike" i]'
        );
        for (const c of candidates) {
            if (c.offsetWidth > 0 || c.offsetHeight > 0 || c.getClientRects().length > 0) {
                return c;
            }
        }
        return candidates[0] || null;
    }

    function getActivationButton(control) {
        if (!control) return null;
        if (control.matches && control.matches('button, tp-yt-paper-button#button')) {
            return control;
        }
        return control.querySelector ? control.querySelector('button, tp-yt-paper-button#button') : null;
    }

    function applyDislikes() {
        try {
            if (!userConfig.return_dislikes) return;
            const videoId = getActiveVideoId();
            if (!videoId) return;

            const meta = videoMetaCache[videoId];
            if (!meta || !meta.dislikes) return;

            const dislikes = meta.dislikes.dislikes;
            const likes = meta.dislikes.likes;
            if (dislikes === undefined || dislikes === null) return;

            const dislikeButtonHost = getDislikeButtonHost();
            if (!dislikeButtonHost) return;

            const actionBtn = getActivationButton(dislikeButtonHost);
            if (!actionBtn) return;

            let label = actionBtn.querySelector('.yt-ryd-label') || dislikeButtonHost.querySelector('.yt-ryd-label');
            const formatted = formatCount(dislikes);

            if (!label) {
                label = document.createElement('div');
                label.className = 'yt-spec-button-shape-next__button-text-content ytSpecButtonShapeNextButtonTextContent yt-ryd-label';
                const span = document.createElement('span');
                span.setAttribute('role', 'text');
                span.textContent = formatted;
                label.appendChild(span);

                const feedback = actionBtn.querySelector('yt-touch-feedback-shape');
                if (feedback && feedback.parentNode === actionBtn) {
                    actionBtn.insertBefore(label, feedback);
                } else {
                    actionBtn.appendChild(label);
                }
                console.log(`[RYD-DISLIKE] ✅ Injected dislike count (${formatted}) into button`);
            } else {
                const span = label.querySelector('span') || label;
                if (span.textContent !== formatted) {
                    span.textContent = formatted;
                }
            }

            actionBtn.classList.remove(
                'yt-spec-button-shape-next--icon-only',
                'ytSpecButtonShapeNextIconOnly',
                'yt-spec-button-shape-next--icon-button',
                'ytSpecButtonShapeNextIconButton'
            );
            actionBtn.classList.add(
                'yt-spec-button-shape-next--icon-leading',
                'ytSpecButtonShapeNextIconLeading',
                'ryd-dislike-button-override'
            );

            actionBtn.style.setProperty('width', 'auto', 'important');
            actionBtn.style.setProperty('min-width', '52px', 'important');
            actionBtn.style.setProperty('padding-right', '14px', 'important');
            actionBtn.style.setProperty('overflow', 'visible', 'important');
            actionBtn.style.setProperty('display', 'inline-flex', 'important');

            let p = actionBtn.parentElement;
            while (p && p.tagName !== 'SEGMENTED-LIKE-DISLIKE-BUTTON-VIEW-MODEL' && p.tagName !== 'YTD-SEGMENTED-LIKE-DISLIKE-BUTTON-RENDERER' && p !== document.body) {
                p.style.setProperty('width', 'auto', 'important');
                p.style.setProperty('min-width', '52px', 'important');
                p.style.setProperty('overflow', 'visible', 'important');
                p = p.parentElement;
            }

            // Ratio Bar under like/dislike segmented container
            const buttonContainer = document.querySelector('segmented-like-dislike-button-view-model') ||
                document.querySelector('ytd-segmented-like-dislike-button-renderer') ||
                document.querySelector('like-button-view-model')?.parentElement?.parentElement;

            if (buttonContainer && (likes !== undefined && dislikes !== undefined)) {
                buttonContainer.style.position = 'relative';
                let bar = buttonContainer.querySelector('#yt-ryd-ratio-bar');
                if (!bar) {
                    bar = document.createElement('div');
                    bar.id = 'yt-ryd-ratio-bar';
                    bar.className = 'yt-ryd-ratio-bar';
                    buttonContainer.appendChild(bar);
                }

                const total = (likes || 0) + (dislikes || 0);
                const likePct = total > 0 ? ((likes / total) * 100).toFixed(1) : 100;
                let fill = bar.querySelector('.yt-ryd-likes-fill');
                if (!fill) {
                    fill = document.createElement('div');
                    fill.className = 'yt-ryd-likes-fill';
                    bar.appendChild(fill);
                }
                fill.style.width = `${likePct}%`;
            }
        } catch (err) {
            console.error('[RYD-DISLIKE] ❌ applyDislikes error: ' + err.name + ': ' + err.message + ' | ' + err.stack);
        }
    }

    // 7. SponsorBlock Auto-Skip & UI Markers
    function showSponsorToast(category) {
        let toast = document.getElementById('yt-sb-toast');
        if (!toast) {
            toast = document.createElement('div');
            toast.id = 'yt-sb-toast';
            toast.className = 'yt-sb-toast';
            const player = document.getElementById('movie_player') || document.body;
            player.appendChild(toast);
        }
        const labelMap = {
            sponsor: 'Спонсорская интеграция',
            selfpromo: 'Самореклама / подписка',
            interaction: 'Просьба лайка / подписки',
            intro: 'Интро / заставка',
            outro: 'Аутро'
        };
        const prefix = document.createElement('span');
        prefix.textContent = '⏩ Пропущено: ';
        const catBold = document.createElement('b');
        catBold.textContent = labelMap[category] || category;
        toast.replaceChildren(prefix, catBold);
        toast.style.display = 'flex';
        clearTimeout(toast._timer);
        toast._timer = setTimeout(() => { toast.style.display = 'none'; }, 2200);
    }

    function checkSponsorSkip() {
        try {
            if (!userConfig.sponsor_block) return;
            const videoId = getActiveVideoId();
            if (!videoId) return;

            const meta = videoMetaCache[videoId];
            if (!meta || !meta.segments || !meta.segments.length) return;

            const video = document.querySelector('video');
            if (!video || video.paused) return;

            const ct = video.currentTime;
            for (const seg of meta.segments) {
                const [start, end] = seg.segment;
                if (ct >= start && ct < (end - 0.2)) {
                    if (lastSkippedUuid !== seg.UUID) {
                        lastSkippedUuid = seg.UUID;
                        video.currentTime = end;
                        showSponsorToast(seg.category);
                        console.log(`[SPONSOR-BLOCK] ⏩ Auto-skipped ${seg.category} (${start.toFixed(1)}s -> ${end.toFixed(1)}s)`);
                        break;
                    }
                }
            }
        } catch (err) {
            console.error('[SPONSOR-BLOCK] ❌ checkSponsorSkip error: ' + (err.stack || err.message || String(err)));
        }
    }

    function getProgressBar() {
        return document.querySelector('.ytp-progress-bar') ||
               document.querySelector('.ytChapteredProgressBarHost') ||
               document.querySelector('.YtProgressBarLineHost') ||
               document.querySelector('.ytp-progress-list');
    }

    function renderSponsorSegments() {
        try {
            if (!userConfig.sponsor_block) return;
            const videoId = getActiveVideoId();
            if (!videoId) return;

            const meta = videoMetaCache[videoId];
            if (!meta || !meta.segments || !meta.segments.length) return;

            const progressBar = getProgressBar();
            const video = document.querySelector('video');
            if (!progressBar) return;
            if (!video || !isFinite(video.duration) || video.duration <= 0) return;

            let container = progressBar.querySelector('#yt-sb-previewbar');
            if (!container) {
                container = document.createElement('div');
                container.id = 'yt-sb-previewbar';
                progressBar.insertBefore(container, progressBar.firstChild);
            }

            if (container.dataset.renderedVideoId === videoId && container.children.length === meta.segments.length) {
                return;
            }

            container.replaceChildren();
            container.dataset.renderedVideoId = videoId;
            const duration = video.duration;

            for (const seg of meta.segments) {
                const [start, end] = seg.segment;
                const leftPct = Math.max(0, Math.min(100, (start / duration) * 100));
                const widthPct = Math.max(0.2, Math.min(100 - leftPct, ((end - start) / duration) * 100));

                const marker = document.createElement('div');
                marker.className = `yt-sb-segment ${seg.category}`;
                marker.style.left = `${leftPct}%`;
                marker.style.width = `${widthPct}%`;
                container.appendChild(marker);
            }
            console.log(`[SPONSOR-BLOCK] 🎨 Rendered ${meta.segments.length} segment markers on timeline (${videoId})`);
        } catch (err) {
            console.error('[SPONSOR-BLOCK] ❌ renderSponsorSegments error: ' + err.name + ': ' + err.message + ' | ' + err.stack);
        }
    }

    function hookVideoEvents() {
        const video = document.querySelector('video');
        if (video && !video.__sbHooked) {
            video.__sbHooked = true;
            video.addEventListener('timeupdate', checkSponsorSkip);
            video.addEventListener('durationchange', renderSponsorSegments);
            video.addEventListener('loadedmetadata', renderSponsorSegments);
        }
    }

    // 8. Max Quality Enforcer
    function enforceMaxQuality() {
        if (!userConfig.max_quality) return;
        try {
            const player = document.getElementById('movie_player');
            if (player && typeof player.getPlaybackQuality === 'function' && typeof player.setPlaybackQualityRange === 'function') {
                const current = player.getPlaybackQuality();
                if (current && (current === 'tiny' || current === 'small' || current === 'medium' || current === 'large')) {
                    player.setPlaybackQualityRange('highres');
                }
            }
        } catch (_) {}
    }

    // 9. Auto-Confirm "Still Watching?"
    function handleStillWatching() {
        if (!userConfig.auto_confirm_watching) return;
        try {
            const confirmBtn = document.querySelector('yt-confirm-dialog-renderer #confirm-button button') ||
                document.querySelector('#confirm-button.yt-confirm-dialog-renderer');
            if (confirmBtn) {
                confirmBtn.click();
            }
        } catch (_) {}
    }

    // 10. Picture-in-Picture
    async function togglePiP() {
        if (!userConfig.enable_pip) return;
        const video = document.querySelector('video');
        if (!video) return;

        try {
            if (document.pictureInPictureElement) {
                await document.exitPictureInPicture();
            } else {
                await video.requestPictureInPicture();
            }
        } catch (_) {}
    }

    function injectPiPButton() {
        if (!userConfig.enable_pip || document.getElementById('yt-desktop-pip-btn')) return;
        const rightControls = document.querySelector('.ytp-right-controls');
        if (!rightControls) return;

        const pipBtn = document.createElement('button');
        pipBtn.id = 'yt-desktop-pip-btn';
        pipBtn.className = 'ytp-button yt-desktop-pip-btn';
        pipBtn.title = 'Picture-in-Picture (Cmd+Shift+P)';
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.setAttribute('viewBox', '0 0 24 24');
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', 'M19 7h-8v6h8V7zm2-4H3c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h18c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16.01H3V4.99h18v14.02z');
        svg.appendChild(path);
        pipBtn.replaceChildren(svg);
        pipBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            togglePiP();
        });

        const fsBtn = rightControls.querySelector('.ytp-fullscreen-button');
        if (fsBtn) {
            rightControls.insertBefore(pipBtn, fsBtn);
        } else {
            rightControls.appendChild(pipBtn);
        }
    }

    // 11. Title Sync
    let lastReportedTitle = '';
    function syncTitle() {
        try {
            const title = document.title;
            if (title && title !== lastReportedTitle) {
                lastReportedTitle = title;
                postIpc({ type: 'title', value: title });
            }
        } catch (_) {}
    }

    // 12. Apple-Grade Glass Control Center HUD & Pill
    function renderGlassPill() {
        const mastheadButtons = document.querySelector('#masthead #end #buttons') ||
                                document.querySelector('ytd-masthead #end #buttons') ||
                                document.querySelector('#masthead #end') ||
                                document.querySelector('ytd-masthead #end');

        let pill = document.getElementById('yt-desktop-pill');
        if (!pill) {
            pill = document.createElement('div');
            pill.id = 'yt-desktop-pill';
            pill.setAttribute('title', 'Панель управления yt-desktop (⌘,)');

            const dot = document.createElement('div');
            dot.className = 'yt-pill-dot';
            dot.id = 'yt-pill-dot';

            const label = document.createElement('span');
            label.className = 'yt-pill-label';
            label.id = 'yt-pill-label';
            label.textContent = '⚡ Anti-DPI';

            const ping = document.createElement('span');
            ping.className = 'yt-pill-ping';
            ping.id = 'yt-pill-ping';
            ping.textContent = '•';

            const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            svg.setAttribute('class', 'yt-pill-gear');
            svg.setAttribute('viewBox', '0 0 24 24');
            const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            path.setAttribute('d', 'M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z');
            svg.appendChild(path);

            pill.appendChild(dot);
            pill.appendChild(label);
            pill.appendChild(ping);
            pill.appendChild(svg);

            pill.addEventListener('click', (e) => {
                e.stopPropagation();
                window.__toggleHud();
            });
        }

        if (mastheadButtons) {
            if (pill.parentElement !== mastheadButtons) {
                mastheadButtons.insertBefore(pill, mastheadButtons.firstChild);
            }
        } else if (document.body && pill.parentElement !== document.body) {
            document.body.appendChild(pill);
        }
    }

    function positionHud() {
        const pill = document.getElementById('yt-desktop-pill');
        const hud = document.getElementById('yt-desktop-hud');
        if (!pill || !hud) return;
        const rect = pill.getBoundingClientRect();
        hud.style.setProperty('top', `${Math.max(12, Math.round(rect.bottom + 8))}px`, 'important');
        const rightOffset = Math.max(16, Math.round(window.innerWidth - rect.right));
        hud.style.setProperty('right', `${rightOffset}px`, 'important');
        hud.style.setProperty('left', 'auto', 'important');
    }

    window.addEventListener('resize', () => {
        const hud = document.getElementById('yt-desktop-hud');
        if (hud && hud.classList.contains('open')) {
            positionHud();
        }
    });

    function updateActiveStrategyChip(activeId) {
        const chips = document.querySelectorAll('.yt-strat-chip');
        for (const chip of chips) {
            if (chip.dataset.strategyId === activeId) {
                chip.classList.add('active');
            } else {
                chip.classList.remove('active');
            }
        }
    }

    function renderControlCenterHud() {
        if (!document.body) return;
        let hud = document.getElementById('yt-desktop-hud');
        if (!hud) {
            hud = document.createElement('div');
            hud.id = 'yt-desktop-hud';

            // Header
            const header = document.createElement('div');
            header.className = 'yt-hud-header';

            const titleBox = document.createElement('div');
            titleBox.className = 'yt-hud-title-box';
            const title = document.createElement('div');
            title.className = 'yt-hud-title';
            title.textContent = 'Управление YouTube';
            const subtitle = document.createElement('div');
            subtitle.className = 'yt-hud-subtitle';
            subtitle.id = 'yt-hud-subtitle';
            subtitle.textContent = 'Anti-DPI • en0 bypass';
            titleBox.appendChild(title);
            titleBox.appendChild(subtitle);

            const benchBtn = document.createElement('button');
            benchBtn.className = 'yt-hud-bench-btn';
            benchBtn.id = 'yt-hud-bench-btn';
            benchBtn.textContent = '⚡ Тест сети';
            benchBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                benchBtn.textContent = '⏳ Тестирую...';
                benchBtn.disabled = true;
                const dot = document.getElementById('yt-pill-dot');
                if (dot) dot.classList.add('busy');
                postIpc({ type: 'run_dpi_benchmark' });
            });

            header.appendChild(titleBox);
            header.appendChild(benchBtn);
            hud.appendChild(header);

            // Section 1: DPI Strategies Grid
            const stratSec = document.createElement('div');
            const stratLabel = document.createElement('div');
            stratLabel.className = 'yt-hud-section-label';
            stratLabel.textContent = 'Стратегия обхода DPI';
            stratSec.appendChild(stratLabel);

            const stratGrid = document.createElement('div');
            stratGrid.className = 'yt-hud-strategies-grid';
            stratGrid.id = 'yt-hud-strategies-grid';

            const strategies = [
                { id: 'auto', name: '⚡ Авто-выбор', ping: '' },
                { id: 'rostelecom', name: 'Ростелеком / Дом.ру', ping: '' },
                { id: 'mts_megafon', name: 'МТС / Мегафон / Yota', ping: '' },
                { id: 'beeline_mgts', name: 'Билайн / МГТС / Теле2', ping: '' },
                { id: 'tlsrec_midsld', name: 'TLS Record (SLD)', ping: '' },
                { id: 'direct', name: 'Прямое (без DPI)', ping: '' },
            ];

            for (const s of strategies) {
                const chip = document.createElement('div');
                chip.className = 'yt-strat-chip';
                chip.dataset.strategyId = s.id;

                const nameEl = document.createElement('div');
                nameEl.className = 'yt-strat-name';
                nameEl.textContent = s.name;

                const pingEl = document.createElement('div');
                pingEl.className = 'yt-strat-ping';
                pingEl.textContent = s.ping || 'готов';

                chip.appendChild(nameEl);
                chip.appendChild(pingEl);

                chip.addEventListener('click', (e) => {
                    e.stopPropagation();
                    postIpc({ type: 'set_dpi_strategy', strategy_id: s.id });
                    updateActiveStrategyChip(s.id);
                });

                stratGrid.appendChild(chip);
            }
            stratSec.appendChild(stratGrid);
            hud.appendChild(stratSec);

            // Section 2: Features Toggles
            const togSec = document.createElement('div');
            const togLabel = document.createElement('div');
            togLabel.className = 'yt-hud-section-label';
            togLabel.textContent = 'Функции и расширения';
            togSec.appendChild(togLabel);

            const togglesList = document.createElement('div');
            togglesList.className = 'yt-hud-toggles';
            togglesList.id = 'yt-hud-toggles';

            const toggleDefs = [
                { key: 'block_ads', icon: '🛡️', title: 'Блокировка рекламы' },
                { key: 'sponsor_block', icon: '⏩', title: 'SponsorBlock (пропуск спонсоров)' },
                { key: 'return_dislikes', icon: '👎', title: 'Return Dislike (счётчик дизлайков)' },
                { key: 'block_shorts', icon: '🚫', title: 'Скрыть Shorts ленту' },
                { key: 'max_quality', icon: '🎬', title: 'Max Quality (автомакс 4K/8K)' },
                { key: 'oled_black', icon: '🖤', title: 'OLED Pure Black (истинный черный)' },
                { key: 'enable_pip', icon: '🪟', title: 'Режим «Картинка в картинке»' },
                { key: 'auto_confirm_watching', icon: '☕', title: 'Не прерывать (Still Watching)' },
            ];

            for (const t of toggleDefs) {
                const row = document.createElement('div');
                row.className = 'yt-hud-toggle-row';

                const info = document.createElement('div');
                info.className = 'yt-toggle-info';
                const icon = document.createElement('div');
                icon.className = 'yt-toggle-icon';
                icon.textContent = t.icon;
                const textBox = document.createElement('div');
                textBox.className = 'yt-toggle-text';
                const titleEl = document.createElement('div');
                titleEl.className = 'yt-toggle-title';
                titleEl.textContent = t.title;
                textBox.appendChild(titleEl);
                info.appendChild(icon);
                info.appendChild(textBox);

                const switchLabel = document.createElement('label');
                switchLabel.className = 'yt-apple-switch';
                const input = document.createElement('input');
                input.type = 'checkbox';
                input.dataset.key = t.key;
                input.checked = !!userConfig[t.key];
                input.addEventListener('change', () => {
                    userConfig[t.key] = input.checked;
                    window.__applyConfig(userConfig);
                    postIpc({ type: 'save_config', config: userConfig });
                });

                const slider = document.createElement('span');
                slider.className = 'yt-apple-slider';

                switchLabel.appendChild(input);
                switchLabel.appendChild(slider);

                row.appendChild(info);
                row.appendChild(switchLabel);
                togglesList.appendChild(row);
            }
            togSec.appendChild(togglesList);
            hud.appendChild(togSec);

            // Footer
            const footer = document.createElement('div');
            footer.className = 'yt-hud-footer';

            const shortcut = document.createElement('span');
            shortcut.textContent = 'Горячая клавиша: ⌘,';

            const linksBox = document.createElement('div');
            linksBox.style.cssText = 'display:flex;gap:12px;align-items:center;';

            const openWelcomeBtn = document.createElement('button');
            openWelcomeBtn.className = 'yt-hud-folder-btn';
            openWelcomeBtn.textContent = '👋 Мастер настройки';
            openWelcomeBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                hud.classList.remove('open');
                window.__showHelloScreen();
            });

            const openFolderBtn = document.createElement('button');
            openFolderBtn.className = 'yt-hud-folder-btn';
            openFolderBtn.textContent = '📁 Конфиг';
            openFolderBtn.addEventListener('click', (e) => {
                e.stopPropagation();
                postIpc({ type: 'open_config_folder' });
            });

            linksBox.appendChild(openWelcomeBtn);
            linksBox.appendChild(openFolderBtn);

            footer.appendChild(shortcut);
            footer.appendChild(linksBox);
            hud.appendChild(footer);

            document.body.appendChild(hud);

            // Close on click outside
            document.addEventListener('click', (e) => {
                if (hud.classList.contains('open') && !hud.contains(e.target) && !document.getElementById('yt-desktop-pill')?.contains(e.target)) {
                    hud.classList.remove('open');
                }
            });

            // Close on Esc
            window.addEventListener('keydown', (e) => {
                if (e.key === 'Escape' && hud.classList.contains('open')) {
                    hud.classList.remove('open');
                }
            });
        }
    }

    window.__toggleHud = function() {
        renderGlassPill();
        renderControlCenterHud();
        const hud = document.getElementById('yt-desktop-hud');
        if (hud) {
            hud.classList.toggle('open');
            if (hud.classList.contains('open')) {
                positionHud();
                postIpc({ type: 'get_hud_state' });
            }
        }
    };

    window.__onHudState = function(cfg, status, isFirstRun) {
        try {
            renderGlassPill();
            renderControlCenterHud();
            const hud = document.getElementById('yt-desktop-hud');
            if (hud && hud.classList.contains('open')) {
                positionHud();
            }
            if (isFirstRun) {
                window.__showHelloScreen();
            }

            if (cfg) {
                window.__applyConfig(cfg);
                const inputs = document.querySelectorAll('#yt-hud-toggles input[type="checkbox"]');
                for (const input of inputs) {
                    const k = input.dataset.key;
                    if (k && cfg[k] !== undefined) {
                        input.checked = !!cfg[k];
                    }
                }
            }

            if (status) {
                const strategyMap = {
                    'auto': 'Авто',
                    'rostelecom': 'Ростелеком',
                    'mts_megafon': 'МТС / Мегафон',
                    'beeline_mgts': 'Билайн / МГТС',
                    'tlsrec_midsld': 'TLS Record',
                    'tlsrec_sni': 'TLS SNI',
                    'direct': 'Прямое'
                };

                const activeId = status.active_strategy || 'rostelecom';
                const selectedId = status.selected_strategy || 'auto';
                const activeName = strategyMap[activeId] || activeId;

                const pillLabel = document.getElementById('yt-pill-label');
                if (pillLabel) {
                    if (selectedId === 'auto') {
                        pillLabel.textContent = `⚡ Авто (${activeName})`;
                    } else {
                        pillLabel.textContent = `⚡ ${activeName}`;
                    }
                }

                const pillPing = document.getElementById('yt-pill-ping');
                if (pillPing) {
                    if (status.last_latency_ms != null && status.last_latency_ms > 0) {
                        pillPing.textContent = `${status.last_latency_ms} ms`;
                    } else {
                        pillPing.textContent = 'активен';
                    }
                }

                updateActiveStrategyChip(selectedId);
            }
        } catch (err) {
            console.error('[HUD] ❌ Error in __onHudState: ' + (err.stack || err.message || String(err)));
        }
    };

    window.__onBenchmarkDone = function(results, status) {
        try {
            const benchBtn = document.getElementById('yt-hud-bench-btn');
            if (benchBtn) {
                benchBtn.textContent = '⚡ Тест сети';
                benchBtn.disabled = false;
            }
            const dot = document.getElementById('yt-pill-dot');
            if (dot) dot.classList.remove('busy');

            if (Array.isArray(results)) {
                for (const r of results) {
                    const chip = document.querySelector(`.yt-strat-chip[data-strategy-id="${r.strategy_id}"]`);
                    if (chip) {
                        const pingEl = chip.querySelector('.yt-strat-ping');
                        if (pingEl) {
                            if (r.success && r.latency_ms != null) {
                                pingEl.textContent = `${r.latency_ms} ms`;
                                pingEl.style.color = '#4ade80';
                            } else {
                                pingEl.textContent = 'сбой';
                                pingEl.style.color = '#ef4444';
                            }
                        }
                    }
                }
            }
            if (status) {
                window.__onHudState(null, status);
            }
        } catch (err) {
            console.error('[HUD] ❌ Error in __onBenchmarkDone: ' + (err.stack || err.message || String(err)));
        }
    };

    // 13. Apple-Grade Welcome / Hello Screen (Onboarding)
    const I18N = {
        ru: {
            title: "YouTube Desktop",
            subtitle: "Сверхлегкий плеер без рекламы и ограничений скорости",
            mode_title: "Режим подключения:",
            badge_bypass: "Для РФ",
            name_bypass: "Россия • Обход DPI",
            desc_bypass: "Встроенный zapret (midsld/oob/disorder). Обходит замедление и блокировки ТСПУ без внешних VPN.",
            badge_direct: "Весь мир",
            name_direct: "Global • Direct Connection",
            desc_direct: "Прямой доступ на максимальной скорости без прокси. Для Европы, США и стран без цензуры.",
            features_title: "Включенные возможности:",
            feat_adblock: "Блокировка рекламы",
            feat_sponsor: "SponsorBlock (пропуск)",
            feat_dislikes: "Возврат дизлайков",
            feat_shorts: "Скрыть Shorts ленту",
            feat_quality: "Автомакс 4K/8K",
            feat_oled: "OLED Pure Black",
            btn_start: "Начать просмотр",
        },
        en: {
            title: "YouTube Desktop",
            subtitle: "Ultra-lightweight native client with adblock & enhancements",
            mode_title: "Choose Connection Mode:",
            badge_bypass: "For Russia",
            name_bypass: "Russia • DPI Bypass",
            desc_bypass: "Built-in zapret engine. Bypasses TSPU throttling and censorship without external VPNs.",
            badge_direct: "Worldwide",
            name_direct: "Global • Direct Connection",
            desc_direct: "Direct ultra-fast playback with zero proxy overhead. Ideal for US, Europe & censorship-free networks.",
            features_title: "Included Superpowers:",
            feat_adblock: "Built-in AdBlock",
            feat_sponsor: "SponsorBlock (Auto-Skip)",
            feat_dislikes: "Return YouTube Dislike",
            feat_shorts: "Distraction-Free Feed",
            feat_quality: "Auto-Max 4K/8K",
            feat_oled: "OLED Pure Black",
            btn_start: "Get Started",
        }
    };

    function detectDefaultLang() {
        const navLang = (navigator.language || navigator.userLanguage || '').toLowerCase();
        if (navLang.startsWith('ru') || navLang.startsWith('be') || navLang.startsWith('kk') || navLang.startsWith('uk')) {
            return 'ru';
        }
        return 'en';
    }

    let currentWelcomeLang = detectDefaultLang();
    let currentWelcomeMode = currentWelcomeLang === 'ru' ? 'bypass' : 'direct';

    function renderWelcomeScreen() {
        if (!document.body) return;
        let overlay = document.getElementById('yt-desktop-welcome');
        if (overlay) {
            overlay.classList.add('open');
            return;
        }

        overlay = document.createElement('div');
        overlay.id = 'yt-desktop-welcome';

        const card = document.createElement('div');
        card.className = 'yt-welcome-card';

        // Header
        const header = document.createElement('div');
        header.className = 'yt-welcome-header';

        const brand = document.createElement('div');
        brand.className = 'yt-welcome-brand';

        const logo = document.createElement('div');
        logo.className = 'yt-welcome-logo';
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.setAttribute('viewBox', '0 0 24 24');
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', 'M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z');
        svg.appendChild(path);
        logo.appendChild(svg);

        const titles = document.createElement('div');
        titles.className = 'yt-welcome-titles';
        const title = document.createElement('h1');
        title.className = 'yt-welcome-title';
        title.dataset.i18n = 'title';
        title.textContent = I18N[currentWelcomeLang].title;
        const subtitle = document.createElement('p');
        subtitle.className = 'yt-welcome-subtitle';
        subtitle.dataset.i18n = 'subtitle';
        subtitle.textContent = I18N[currentWelcomeLang].subtitle;
        titles.appendChild(title);
        titles.appendChild(subtitle);

        brand.appendChild(logo);
        brand.appendChild(titles);

        // Language switcher
        const langToggle = document.createElement('div');
        langToggle.className = 'yt-welcome-lang-toggle';
        const ruBtn = document.createElement('button');
        ruBtn.type = 'button';
        ruBtn.className = 'yt-lang-btn' + (currentWelcomeLang === 'ru' ? ' active' : '');
        ruBtn.dataset.lang = 'ru';
        ruBtn.textContent = '🇷🇺 RU';
        const enBtn = document.createElement('button');
        enBtn.type = 'button';
        enBtn.className = 'yt-lang-btn' + (currentWelcomeLang === 'en' ? ' active' : '');
        enBtn.dataset.lang = 'en';
        enBtn.textContent = '🇬🇧 EN';

        ruBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            setWelcomeLang('ru');
        });
        enBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            setWelcomeLang('en');
        });

        langToggle.appendChild(ruBtn);
        langToggle.appendChild(enBtn);

        header.appendChild(brand);
        header.appendChild(langToggle);
        card.appendChild(header);

        // Mode Section
        const modeSec = document.createElement('div');
        modeSec.className = 'yt-welcome-section';
        const modeSecTitle = document.createElement('div');
        modeSecTitle.className = 'yt-welcome-sec-title';
        modeSecTitle.dataset.i18n = 'mode_title';
        modeSecTitle.textContent = I18N[currentWelcomeLang].mode_title;
        modeSec.appendChild(modeSecTitle);

        const modesBox = document.createElement('div');
        modesBox.className = 'yt-welcome-modes';

        // Bypass card
        const bypassCard = document.createElement('div');
        bypassCard.className = 'yt-mode-card' + (currentWelcomeMode === 'bypass' ? ' active' : '');
        bypassCard.dataset.mode = 'bypass';

        const bypassBadge = document.createElement('div');
        bypassBadge.className = 'yt-mode-badge bypass';
        bypassBadge.dataset.i18n = 'badge_bypass';
        bypassBadge.textContent = I18N[currentWelcomeLang].badge_bypass;

        const bypassIcon = document.createElement('div');
        bypassIcon.className = 'yt-mode-icon';
        bypassIcon.textContent = '⚡';

        const bypassInfo = document.createElement('div');
        bypassInfo.className = 'yt-mode-info';
        const bypassName = document.createElement('div');
        bypassName.className = 'yt-mode-name';
        bypassName.dataset.i18n = 'name_bypass';
        bypassName.textContent = I18N[currentWelcomeLang].name_bypass;
        const bypassDesc = document.createElement('div');
        bypassDesc.className = 'yt-mode-desc';
        bypassDesc.dataset.i18n = 'desc_bypass';
        bypassDesc.textContent = I18N[currentWelcomeLang].desc_bypass;
        bypassInfo.appendChild(bypassName);
        bypassInfo.appendChild(bypassDesc);

        const bypassRadio = document.createElement('div');
        bypassRadio.className = 'yt-mode-radio';

        bypassCard.appendChild(bypassBadge);
        bypassCard.appendChild(bypassIcon);
        bypassCard.appendChild(bypassInfo);
        bypassCard.appendChild(bypassRadio);

        // Direct card
        const directCard = document.createElement('div');
        directCard.className = 'yt-mode-card' + (currentWelcomeMode === 'direct' ? ' active' : '');
        directCard.dataset.mode = 'direct';

        const directBadge = document.createElement('div');
        directBadge.className = 'yt-mode-badge direct';
        directBadge.dataset.i18n = 'badge_direct';
        directBadge.textContent = I18N[currentWelcomeLang].badge_direct;

        const directIcon = document.createElement('div');
        directIcon.className = 'yt-mode-icon';
        directIcon.textContent = '🚀';

        const directInfo = document.createElement('div');
        directInfo.className = 'yt-mode-info';
        const directName = document.createElement('div');
        directName.className = 'yt-mode-name';
        directName.dataset.i18n = 'name_direct';
        directName.textContent = I18N[currentWelcomeLang].name_direct;
        const directDesc = document.createElement('div');
        directDesc.className = 'yt-mode-desc';
        directDesc.dataset.i18n = 'desc_direct';
        directDesc.textContent = I18N[currentWelcomeLang].desc_direct;
        directInfo.appendChild(directName);
        directInfo.appendChild(directDesc);

        const directRadio = document.createElement('div');
        directRadio.className = 'yt-mode-radio';

        directCard.appendChild(directBadge);
        directCard.appendChild(directIcon);
        directCard.appendChild(directInfo);
        directCard.appendChild(directRadio);

        bypassCard.addEventListener('click', (e) => {
            e.stopPropagation();
            currentWelcomeMode = 'bypass';
            bypassCard.classList.add('active');
            directCard.classList.remove('active');
        });

        directCard.addEventListener('click', (e) => {
            e.stopPropagation();
            currentWelcomeMode = 'direct';
            directCard.classList.add('active');
            bypassCard.classList.remove('active');
        });

        modesBox.appendChild(bypassCard);
        modesBox.appendChild(directCard);
        modeSec.appendChild(modesBox);
        card.appendChild(modeSec);

        // Features Preview Section
        const featSec = document.createElement('div');
        featSec.className = 'yt-welcome-section';
        const featTitle = document.createElement('div');
        featTitle.className = 'yt-welcome-sec-title';
        featTitle.dataset.i18n = 'features_title';
        featTitle.textContent = I18N[currentWelcomeLang].features_title;
        featSec.appendChild(featTitle);

        const chipsGrid = document.createElement('div');
        chipsGrid.className = 'yt-welcome-chips';

        const chips = [
            { icon: '🛡️', key: 'feat_adblock' },
            { icon: '⏩', key: 'feat_sponsor' },
            { icon: '👎', key: 'feat_dislikes' },
            { icon: '🚫', key: 'feat_shorts' },
            { icon: '🎬', key: 'feat_quality' },
            { icon: '🖤', key: 'feat_oled' },
        ];

        for (const c of chips) {
            const chip = document.createElement('div');
            chip.className = 'yt-welcome-chip';
            chip.textContent = c.icon + ' ';
            const span = document.createElement('span');
            span.dataset.i18n = c.key;
            span.textContent = I18N[currentWelcomeLang][c.key];
            chip.appendChild(span);
            chipsGrid.appendChild(chip);
        }
        featSec.appendChild(chipsGrid);
        card.appendChild(featSec);

        // Footer & Submit
        const footer = document.createElement('div');
        footer.className = 'yt-welcome-footer';

        const submitBtn = document.createElement('button');
        submitBtn.type = 'button';
        submitBtn.className = 'yt-welcome-submit-btn';
        submitBtn.id = 'yt-welcome-submit';

        const submitText = document.createElement('span');
        submitText.dataset.i18n = 'btn_start';
        submitText.textContent = I18N[currentWelcomeLang].btn_start;

        const arrow = document.createElement('span');
        arrow.className = 'yt-arrow';
        arrow.textContent = '→';

        submitBtn.appendChild(submitText);
        submitBtn.appendChild(arrow);

        submitBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            const chosenStrategy = currentWelcomeMode === 'direct' ? 'direct' : 'auto';
            userConfig.selected_strategy = chosenStrategy;
            userConfig.first_run_completed = true;
            userConfig.app_locale = currentWelcomeLang;

            postIpc({
                type: 'complete_onboarding',
                strategy_id: chosenStrategy,
                config: userConfig
            });

            overlay.classList.remove('open');
            setTimeout(() => {
                overlay.remove();
            }, 300);
        });

        footer.appendChild(submitBtn);
        card.appendChild(footer);

        overlay.appendChild(card);
        document.body.appendChild(overlay);

        requestAnimationFrame(() => {
            overlay.classList.add('open');
        });
    }

    function setWelcomeLang(lang) {
        if (!I18N[lang]) return;
        currentWelcomeLang = lang;

        const ruBtn = document.querySelector('.yt-lang-btn[data-lang="ru"]');
        const enBtn = document.querySelector('.yt-lang-btn[data-lang="en"]');
        if (ruBtn && enBtn) {
            ruBtn.classList.toggle('active', lang === 'ru');
            enBtn.classList.toggle('active', lang === 'en');
        }

        const elements = document.querySelectorAll('#yt-desktop-welcome [data-i18n]');
        for (const el of elements) {
            const key = el.dataset.i18n;
            if (key && I18N[lang][key]) {
                el.textContent = I18N[lang][key];
            }
        }
    }

    window.__showHelloScreen = function() {
        renderWelcomeScreen();
    };

    // 14. Global Keyboard Shortcuts
    window.addEventListener('keydown', (e) => {
        if (e.metaKey && e.key === ',') {
            e.preventDefault();
            window.__toggleHud();
            return;
        }
        if (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'p') {
            e.preventDefault();
            togglePiP();
            return;
        }
        if (e.metaKey && e.key === '[') {
            e.preventDefault();
            window.history.back();
            return;
        }
        if (e.metaKey && e.key === ']') {
            e.preventDefault();
            window.history.forward();
            return;
        }
        if (e.metaKey && e.key.toLowerCase() === 'r') {
            e.preventDefault();
            window.location.reload();
            return;
        }
    });

    // SPA Navigation Listeners
    window.addEventListener('yt-navigate-finish', () => {
        syncVideoMeta();
        tick();
    });
    window.addEventListener('yt-page-data-updated', () => {
        syncVideoMeta();
        tick();
    });

    // 14. Main Loop & Observers
    function tick() {
        injectStyles();
        renderGlassPill();
        renderControlCenterHud();
        checkShortsRedirect();
        handleVideoAds();
        handleAntiAdblock();
        handleStillWatching();
        injectPiPButton();
        syncTitle();
        enforceMaxQuality();
        hookVideoEvents();

        syncVideoMeta();
        applyDislikes();
        renderSponsorSegments();
        checkSponsorSkip();
    }

    tick();
    setInterval(tick, 150);

    const observer = new MutationObserver(() => tick());
    if (document.body) {
        observer.observe(document.body, { childList: true, subtree: true });
    } else {
        document.addEventListener('DOMContentLoaded', () => {
            tick();
            if (document.body) {
                observer.observe(document.body, { childList: true, subtree: true });
            }
        });
    }

    // Initial HUD state request from Rust
    setTimeout(() => {
        postIpc({ type: 'get_hud_state' });
        if (window.__YT_FIRST_RUN__ === true) {
            window.__showHelloScreen();
        }
    }, 120);

    console.log('[yt-desktop] High-speed extensions engine initialized.');
})();
