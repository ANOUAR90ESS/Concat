// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

package app.concat.editor;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;
import android.os.PowerManager;

/**
 * Keeps a Long video run going while the person is in another app.
 *
 * Android freezes, and in time ends, an app it no longer shows; a run of a
 * long video takes many minutes of waiting on the network, and left alone
 * it stopped as soon as the phone was put down. A foreground service is
 * how an app says it is doing work the person asked for: it shows a
 * notification for as long as it runs, and the process it lives in is kept.
 * It does nothing itself - the run is the app's own threads - but hold a
 * partial wake lock, so the CPU does not sleep with the screen off.
 *
 * Started and stopped from the Rust side through
 * {@link ConcatFiles#keepRunning}. The system makes this class from the
 * APK's own classes.dex, which the CI step that declares it in the
 * manifest puts there (scripts/ci/android-service.py): cargo-apk writes no
 * service and no dex of its own.
 */
public class RunService extends Service {
    /** The class's name, for an intent built from strings. */
    public static final String NAME = "app.concat.editor.RunService";
    /** The intent's words for the notification. */
    public static final String DOING = "doing";
    /** How far, in percent, for the notification's bar; -1 for none. */
    public static final String PROGRESS = "progress";

    private static final String CHANNEL = "run";
    private static final int NOTIFICATION = 1;
    /** Longer than any run, and the most Android 15 gives this kind. */
    private static final long LONGEST = 6L * 60 * 60 * 1000;

    private PowerManager.WakeLock wake;

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String doing = intent == null ? null : intent.getStringExtra(DOING);
        int progress = intent == null ? -1 : intent.getIntExtra(PROGRESS, -1);
        Notification notification = notification(doing == null ? "" : doing, progress);
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(NOTIFICATION, notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC);
        } else {
            startForeground(NOTIFICATION, notification);
        }
        if (wake == null) {
            PowerManager power = (PowerManager) getSystemService(POWER_SERVICE);
            if (power != null) {
                wake = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "longcut:run");
                wake.setReferenceCounted(false);
                wake.acquire(LONGEST);
            }
        }
        // A process that was ended is not brought back for this: the run
        // carries on where it stopped when the app is opened again.
        return START_NOT_STICKY;
    }

    /** Android 15's limit on this kind of service: let go when it comes. */
    public void onTimeout(int startId, int type) {
        stopSelf();
    }

    @Override
    public void onDestroy() {
        if (wake != null && wake.isHeld()) {
            wake.release();
        }
        wake = null;
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    /**
     * The notification the service runs under, with a bar for how far when
     * that is known; a tap opens the app. Said again as the job goes on,
     * quietly: only the first one is announced.
     */
    private Notification notification(String doing, int progress) {
        NotificationManager manager =
                (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
        if (manager != null) {
            NotificationChannel channel = new NotificationChannel(
                    CHANNEL, "Long video", NotificationManager.IMPORTANCE_LOW);
            manager.createNotificationChannel(channel);
        }
        Notification.Builder builder = new Notification.Builder(this, CHANNEL)
                .setContentTitle(getApplicationInfo().loadLabel(getPackageManager()))
                .setContentText(doing)
                .setSmallIcon(getApplicationInfo().icon)
                .setOnlyAlertOnce(true)
                .setOngoing(true);
        if (progress >= 0) {
            builder.setProgress(100, progress, false);
        }
        Intent open = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (open != null) {
            builder.setContentIntent(PendingIntent.getActivity(this, 0, open,
                    PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT));
        }
        return builder.build();
    }
}
