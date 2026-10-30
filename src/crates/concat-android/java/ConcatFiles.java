// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

package app.concat.editor;

import android.app.Activity;
import android.app.Fragment;
import android.content.ClipData;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.util.Log;
import android.util.Range;
import android.view.WindowManager;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;
import java.util.List;

/**
 * The system's document picker, for an activity that has no Java of its
 * own to receive its answer.
 *
 * The window is a NativeActivity: the framework's own class, with nothing
 * of ours to override, and a picker's result comes back only through an
 * activity's or a fragment's onActivityResult. So this is a fragment - a
 * headless one, added to the activity for the length of one pick - and
 * the result comes back here.
 *
 * What is picked is a content URI, which the engine's decoder cannot open:
 * it reads files by path. Each is copied into the app's own external
 * files, under Imported/, keeping its display name, and the paths are
 * what the engine is handed - through the native method below, which the
 * Rust side registers before it asks for a pick.
 *
 * The way out is here as well: a finished export is written into the
 * app's own folder, which on a phone nothing but the app can open, so
 * {@link #publishVideo} moves it into the phone's Movies through the
 * media store, where the gallery and every file manager show it.
 *
 * Compiled by build.rs with the SDK's javac and d8, and loaded at run time
 * from the dex inside the binary; see src/lib.rs.
 */
public class ConcatFiles extends Fragment {
    private static final int REQUEST_PICK = 0xC0C4;
    private static final String TAG = "concat-files";

    /**
     * Where a finished export goes, under the phone's storage: Movies/Concat.
     * Null before Android 10, which has no media store an app can write
     * through without a permission; there the file stays in the app's folder.
     */
    public static String publishedFolder() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            return null;
        }
        return Environment.DIRECTORY_MOVIES + "/Concat";
    }

    /**
     * Moves the video at {@code path} into {@link #publishedFolder()} through
     * the media store and deletes the original. Two strings come back: the
     * file's path under the phone's storage and null, or null and why it
     * could not be moved - in which case the original is still where it was.
     * Blocking: a video is big and the copy takes a while, so this is called
     * off the window's thread.
     */
    public static String[] publishVideo(Activity activity, String path) {
        String folder = publishedFolder();
        if (folder == null) {
            return new String[] {null, "this phone has no media store to put it in"};
        }
        File file = new File(path);
        ContentResolver resolver = activity.getContentResolver();
        ContentValues values = new ContentValues();
        values.put(MediaStore.Video.Media.DISPLAY_NAME, file.getName());
        values.put(MediaStore.Video.Media.MIME_TYPE, "video/mp4");
        values.put(MediaStore.Video.Media.RELATIVE_PATH, folder);
        values.put(MediaStore.Video.Media.IS_PENDING, 1);
        Uri collection = MediaStore.Video.Media.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY);
        Uri uri;
        try {
            uri = resolver.insert(collection, values);
        } catch (Exception e) {
            return new String[] {null, "the media store refused the file: " + e.getMessage()};
        }
        if (uri == null) {
            return new String[] {null, "the media store refused the file"};
        }
        InputStream in = null;
        OutputStream out = null;
        try {
            in = new FileInputStream(file);
            out = resolver.openOutputStream(uri);
            if (out == null) {
                throw new IOException("the media store would not open the file for writing");
            }
            byte[] buffer = new byte[1 << 16];
            int read;
            while ((read = in.read(buffer)) > 0) {
                out.write(buffer, 0, read);
            }
            out.close();
            out = null;
            values.clear();
            values.put(MediaStore.Video.Media.IS_PENDING, 0);
            resolver.update(uri, values, null, null);
        } catch (Exception e) {
            try {
                resolver.delete(uri, null, null);
            } catch (Exception ignored) {
            }
            return new String[] {null, e.getMessage() == null ? e.toString() : e.getMessage()};
        } finally {
            try {
                if (in != null) in.close();
            } catch (IOException ignored) {
            }
            try {
                if (out != null) out.close();
            } catch (IOException ignored) {
            }
        }
        file.delete();
        lastPublished = uri;
        // The store may have renamed it: a second "name.mp4" is "name (1).mp4".
        return new String[] {folder + "/" + displayName(activity, uri), null};
    }

    /** Registered from Rust: the picked files' paths, or none. */
    public static native void filesPicked(String[] paths);

    /** Shows the picker. From any thread; the fragment is added on the UI thread. */
    public static void pick(final Activity activity) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                ConcatFiles fragment = new ConcatFiles();
                activity.getFragmentManager()
                        .beginTransaction()
                        .add(fragment, TAG)
                        .commitAllowingStateLoss();
                activity.getFragmentManager().executePendingTransactions();

                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                intent.setType("*/*");
                intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {"video/*", "audio/*", "image/*"});
                intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
                try {
                    fragment.startActivityForResult(intent, REQUEST_PICK);
                } catch (Exception e) {
                    fragment.done(new ArrayList<Uri>());
                }
            }
        });
    }

    @Override
    public void onActivityResult(int request, int result, Intent data) {
        if (request != REQUEST_PICK) {
            return;
        }
        List<Uri> uris = new ArrayList<Uri>();
        if (result == Activity.RESULT_OK && data != null) {
            ClipData clip = data.getClipData();
            if (clip != null) {
                for (int i = 0; i < clip.getItemCount(); i++) {
                    Uri uri = clip.getItemAt(i).getUri();
                    if (uri != null) {
                        uris.add(uri);
                    }
                }
            } else if (data.getData() != null) {
                uris.add(data.getData());
            }
        }
        done(uris);
    }

    /** Copies what was picked and reports it; then the fragment goes. */
    private void done(final List<Uri> uris) {
        final Activity activity = getActivity();
        try {
            getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
        } catch (Exception ignored) {
        }
        if (activity == null || uris.isEmpty()) {
            filesPicked(new String[0]);
            return;
        }
        // Off the UI thread: a video is big and the copy takes a while.
        new Thread(new Runnable() {
            @Override
            public void run() {
                List<String> paths = new ArrayList<String>();
                File dir = new File(activity.getExternalFilesDir(null), "Imported");
                dir.mkdirs();
                for (Uri uri : uris) {
                    File out = unique(dir, displayName(activity, uri));
                    if (copy(activity, uri, out)) {
                        paths.add(out.getAbsolutePath());
                    }
                }
                filesPicked(paths.toArray(new String[0]));
            }
        }, TAG).start();
    }

    /** The name the picker showed for the file, or one made from the URI. */
    private static String displayName(Activity activity, Uri uri) {
        String name = null;
        Cursor cursor = null;
        try {
            cursor = activity.getContentResolver().query(uri, null, null, null, null);
            if (cursor != null && cursor.moveToFirst()) {
                int column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (column >= 0) {
                    name = cursor.getString(column);
                }
            }
        } catch (Exception ignored) {
        } finally {
            if (cursor != null) {
                cursor.close();
            }
        }
        if (name == null || name.isEmpty()) {
            name = uri.getLastPathSegment();
        }
        if (name == null || name.isEmpty()) {
            name = "import";
        }
        // Whatever the provider called it, it is one file name.
        return name.replace('/', '_').replace('\\', '_');
    }

    /** `name`, or `name (2)`, `name (3)`... until nothing is overwritten. */
    private static File unique(File dir, String name) {
        File out = new File(dir, name);
        if (!out.exists()) {
            return out;
        }
        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String ext = dot > 0 ? name.substring(dot) : "";
        for (int n = 2; ; n++) {
            out = new File(dir, stem + " (" + n + ")" + ext);
            if (!out.exists()) {
                return out;
            }
        }
    }

    private static boolean copy(Activity activity, Uri uri, File out) {
        InputStream in = null;
        OutputStream os = null;
        try {
            in = activity.getContentResolver().openInputStream(uri);
            if (in == null) {
                return false;
            }
            os = new FileOutputStream(out);
            byte[] buffer = new byte[1 << 16];
            int read;
            while ((read = in.read(buffer)) > 0) {
                os.write(buffer, 0, read);
            }
            return true;
        } catch (IOException e) {
            out.delete();
            return false;
        } finally {
            try {
                if (in != null) in.close();
            } catch (IOException ignored) {
            }
            try {
                if (os != null) os.close();
            } catch (IOException ignored) {
            }
        }
    }

    /** The last video put in the media store, to play or share. */
    private static Uri lastPublished;

    /**
     * Opens the last video put in the media store in the phone's own player.
     * False when there is none, or nothing on the phone plays it.
     */
    public static boolean playVideo(Activity activity) {
        if (lastPublished == null) {
            return false;
        }
        Intent intent = new Intent(Intent.ACTION_VIEW)
                .setDataAndType(lastPublished, "video/mp4")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try {
            activity.startActivity(intent);
            return true;
        } catch (RuntimeException e) {
            Log.w(TAG, "could not play the video", e);
            return false;
        }
    }

    /**
     * Offers the last video put in the media store to the apps that take
     * one - YouTube, a messenger, the cloud - through the system's sheet.
     */
    public static boolean shareVideo(Activity activity) {
        if (lastPublished == null) {
            return false;
        }
        Intent send = new Intent(Intent.ACTION_SEND)
                .setType("video/mp4")
                .putExtra(Intent.EXTRA_STREAM, lastPublished)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try {
            activity.startActivity(Intent.createChooser(send, null));
            return true;
        } catch (RuntimeException e) {
            Log.w(TAG, "could not share the video", e);
            return false;
        }
    }

<<<<<<< HEAD
    /**
     * Hands text - the app's log - to the apps that take it: a messenger,
     * mail, a note. Headed by the phone's name and its video encoders, the
     * two things a report from a phone is read for first. True when
     * something was offered it.
     */
    public static boolean shareText(Activity activity, String subject, String text) {
        String head = device() + "\n" + encoders() + "\n";
        Intent send = new Intent(Intent.ACTION_SEND)
                .setType("text/plain")
                .putExtra(Intent.EXTRA_SUBJECT, subject)
                .putExtra(Intent.EXTRA_TEXT, head + "\n" + text);
        try {
            activity.startActivity(Intent.createChooser(send, null));
            return true;
        } catch (RuntimeException e) {
            Log.w(TAG, "could not share the log", e);
            return false;
        }
    }

    /** The phone's maker, model and Android version, on one line. */
    public static String device() {
        return Build.MANUFACTURER + " " + Build.MODEL + ", Android " + Build.VERSION.RELEASE
                + " (API " + Build.VERSION.SDK_INT + ")";
    }

    /**
     * This phone's H.264 and HEVC encoders, one a line, in the order
     * Android picks them - the first of a kind is the one an export gets:
     * "video/avc c2.qti.avc.encoder hw cq=0-100 vbr cbr". The window's
     * crate reads whether the first takes a constant quality, and in what
     * range, out of these lines.
     */
    public static String encoders() {
        StringBuilder lines = new StringBuilder();
        String[] kinds = {"video/avc", "video/hevc"};
        try {
            MediaCodecInfo[] infos = new MediaCodecList(MediaCodecList.REGULAR_CODECS).getCodecInfos();
            for (String kind : kinds) {
                for (MediaCodecInfo info : infos) {
                    if (!info.isEncoder() || !supports(info, kind)) {
                        continue;
                    }
                    lines.append(kind).append(' ').append(info.getName());
                    if (Build.VERSION.SDK_INT >= 29) {
                        lines.append(info.isHardwareAccelerated() ? " hw" : " sw");
                    }
                    MediaCodecInfo.EncoderCapabilities encoder =
                            info.getCapabilitiesForType(kind).getEncoderCapabilities();
                    if (encoder.isBitrateModeSupported(
                            MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_CQ)) {
                        lines.append(" cq");
                        if (Build.VERSION.SDK_INT >= 28) {
                            Range<Integer> quality = encoder.getQualityRange();
                            lines.append('=').append(quality.getLower()).append('-')
                                    .append(quality.getUpper());
                        }
                    }
                    if (encoder.isBitrateModeSupported(
                            MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_VBR)) {
                        lines.append(" vbr");
                    }
                    if (encoder.isBitrateModeSupported(
                            MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_CBR)) {
                        lines.append(" cbr");
                    }
                    lines.append('\n');
                }
            }
        } catch (RuntimeException e) {
            Log.w(TAG, "could not list the encoders", e);
        }
        return lines.toString().trim();
    }

    private static boolean supports(MediaCodecInfo info, String kind) {
        for (String type : info.getSupportedTypes()) {
            if (type.equalsIgnoreCase(kind)) {
                return true;
            }
        }
        return false;
    }

=======
>>>>>>> origin/ccr-e4840680-erqz1g
    /** Asked for the notification permission once a process, not every run. */
    private static boolean askedToNotify;

    /**
     * Keeps a run going while the app is in the background - the words
     * say what it is doing, and `progress` how far in percent, or -1 when
     * that is not known - or, with no words, lets go. Through
     * {@link RunService} where the APK declares it; where it does not, or
     * Android will not start it, the screen is kept on instead, which at
     * least keeps the run going while the app is in front. True when the
     * service took it.
     */
    public static boolean keepRunning(final Activity activity, String doing, int progress) {
        Intent intent = new Intent().setClassName(activity.getPackageName(), RunService.NAME);
        if (doing == null || doing.isEmpty()) {
            try {
                activity.stopService(intent);
            } catch (RuntimeException e) {
                Log.w(TAG, "could not stop the run service", e);
            }
            screenOn(activity, false);
            return true;
        }
        askToNotify(activity);
        intent.putExtra(RunService.DOING, doing);
        intent.putExtra(RunService.PROGRESS, progress);
        try {
            if (activity.startForegroundService(intent) != null) {
                return true;
            }
            Log.i(TAG, "this APK declares no run service");
        } catch (RuntimeException e) {
            Log.w(TAG, "could not start the run service", e);
        }
        screenOn(activity, true);
        return false;
    }

    /**
     * Android 13 shows a service's notification only to an app allowed to
     * post one; asked once, and a no is kept - the service runs either way.
     */
    private static void askToNotify(Activity activity) {
        if (askedToNotify || Build.VERSION.SDK_INT < 33) {
            return;
        }
        askedToNotify = true;
        String permission = "android.permission.POST_NOTIFICATIONS";
        if (activity.checkSelfPermission(permission) != PackageManager.PERMISSION_GRANTED) {
            activity.requestPermissions(new String[] {permission}, REQUEST_PICK + 1);
        }
    }

    private static void screenOn(final Activity activity, final boolean on) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (on) {
                    activity.getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
                } else {
                    activity.getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
                }
            }
        });
    }
}
