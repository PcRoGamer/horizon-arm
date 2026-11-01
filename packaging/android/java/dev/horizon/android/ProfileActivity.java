package dev.horizon.android;

import android.app.Activity;
import android.content.Intent;
import android.graphics.BitmapFactory;
import android.net.Uri;
import android.os.Bundle;
import android.util.Base64;
import android.view.Gravity;
import android.view.View;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.Spinner;
import android.widget.TextView;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URI;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Bedrock-like local profile manager: offline name + reusable skin library. */
public final class ProfileActivity extends Activity {
    private static final int PICK_SKIN = 41;
    private static final String ACCOUNTS = "horizon-offline-accounts.json";
    private static final String LEGACY = "horizon-player.txt";
    private static final String SKIN_DIR = "horizon-profile-skins";
    private static final String ACTIVE_SKIN = "horizon-profile-skin.png";
    private static final String ACTIVE_MODEL = "horizon-profile-skin-model.txt";

    private final ExecutorService network = Executors.newSingleThreadExecutor();
    private final List<Account> accounts = new ArrayList<>();
    private final List<Skin> skins = new ArrayList<>();
    private Spinner accountSpinner;
    private Spinner skinSpinner;
    private EditText name;
    private EditText lookup;
    private CheckBox slim;
    private TextView status;
    private int selectedAccount;

    private static final class Account {
        String name;
        String skin;
        Account(String name, String skin) { this.name = name; this.skin = skin; }
    }

    private static final class Skin {
        String id;
        String label;
        boolean slim;
        Skin(String id, String label, boolean slim) {
            this.id = id; this.label = label; this.slim = slim;
        }
        @Override public String toString() { return label + (slim ? " · slim" : " · classic"); }
    }

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().setStatusBarColor(0xFF111111);
        getWindow().setNavigationBarColor(0xFF111111);
        loadAccounts();
        loadSkins();
        buildUi();
        selectAccount(Math.min(selectedAccount, accounts.size() - 1));
    }

    @Override
    protected void onDestroy() {
        network.shutdownNow();
        super.onDestroy();
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    private TextView heading(String text) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextColor(0xFFFFFFFF);
        view.setTextSize(21);
        view.setPadding(0, dp(10), 0, dp(6));
        return view;
    }

    private TextView label(String text) {
        TextView view = new TextView(this);
        view.setText(text);
        view.setTextColor(0xFFBBBBBB);
        view.setTextSize(14);
        view.setPadding(0, dp(10), 0, dp(4));
        return view;
    }

    private Button button(String text) {
        Button b = new Button(this);
        b.setText(text);
        b.setAllCaps(false);
        return b;
    }

    private void buildUi() {
        ScrollView scroll = new ScrollView(this);
        scroll.setBackgroundColor(0xFF111111);
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(dp(20), dp(14), dp(20), dp(24));
        scroll.addView(root, new ScrollView.LayoutParams(
                ScrollView.LayoutParams.MATCH_PARENT, ScrollView.LayoutParams.WRAP_CONTENT));

        TextView title = heading("Profile");
        title.setTextSize(28);
        root.addView(title);
        TextView intro = label("Offline identity and skin. Nothing here signs into Microsoft or stores a password.");
        intro.setPadding(0, 0, 0, dp(12));
        root.addView(intro);

        root.addView(label("Saved profiles"));
        accountSpinner = new Spinner(this);
        root.addView(accountSpinner, match());
        accountSpinner.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(android.widget.AdapterView<?> parent, View view, int position, long id) {
                if (position >= 0 && position < accounts.size()) selectAccount(position);
            }
            @Override public void onNothingSelected(android.widget.AdapterView<?> parent) {}
        });

        root.addView(label("Player name"));
        name = new EditText(this);
        name.setSingleLine(true);
        name.setTextColor(0xFFFFFFFF);
        name.setHintTextColor(0xFF777777);
        name.setHint("3–16 letters, numbers or _");
        root.addView(name, match());

        LinearLayout profileActions = row();
        Button save = button("Save / Select");
        Button add = button("New");
        Button delete = button("Delete");
        profileActions.addView(save, weight());
        profileActions.addView(add, weight());
        profileActions.addView(delete, weight());
        root.addView(profileActions, match());
        save.setOnClickListener(v -> saveCurrent());
        add.setOnClickListener(v -> newProfile());
        delete.setOnClickListener(v -> deleteCurrent());

        root.addView(heading("Skin"));
        root.addView(label("Saved skins"));
        skinSpinner = new Spinner(this);
        root.addView(skinSpinner, match());
        skinSpinner.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(android.widget.AdapterView<?> parent, View view, int position, long id) {
                Skin skin = skinAt(position);
                slim.setChecked(skin != null && skin.slim);
            }
            @Override public void onNothingSelected(android.widget.AdapterView<?> parent) {}
        });

        slim = new CheckBox(this);
        slim.setText("Slim arms (Alex model)");
        slim.setTextColor(0xFFFFFFFF);
        root.addView(slim);

        LinearLayout skinActions = row();
        Button importSkin = button("Import PNG");
        Button resetSkin = button("Default skin");
        skinActions.addView(importSkin, weight());
        skinActions.addView(resetSkin, weight());
        root.addView(skinActions, match());
        importSkin.setOnClickListener(v -> pickSkin());
        resetSkin.setOnClickListener(v -> {
            skinSpinner.setSelection(0);
            slim.setChecked(false);
            status("Default skin selected. Tap Save / Select.");
        });

        root.addView(label("Copy the official skin of a Minecraft username"));
        lookup = new EditText(this);
        lookup.setSingleLine(true);
        lookup.setTextColor(0xFFFFFFFF);
        lookup.setHintTextColor(0xFF777777);
        lookup.setHint("Minecraft username");
        root.addView(lookup, match());
        Button download = button("Download skin from username");
        root.addView(download, match());
        download.setOnClickListener(v -> downloadUsernameSkin());

        status = new TextView(this);
        status.setTextColor(0xFF9FD3FF);
        status.setTextSize(14);
        status.setGravity(Gravity.CENTER_HORIZONTAL);
        status.setPadding(0, dp(16), 0, dp(12));
        root.addView(status, match());

        Button done = button("Back to Horizon");
        done.setOnClickListener(v -> finish());
        root.addView(done, match());
        setContentView(scroll);
        refreshAccountSpinner();
        refreshSkinSpinner(null);
    }

    private LinearLayout.LayoutParams match() {
        return new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT);
    }

    private LinearLayout.LayoutParams weight() {
        return new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1);
    }

    private LinearLayout row() {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        return row;
    }

    private boolean validName(String value) {
        return value != null && value.matches("[A-Za-z0-9_]{3,16}");
    }

    private void refreshAccountSpinner() {
        List<String> names = new ArrayList<>();
        for (Account account : accounts) names.add(account.name);
        accountSpinner.setAdapter(new ArrayAdapter<>(this,
                android.R.layout.simple_spinner_dropdown_item, names));
    }

    private void refreshSkinSpinner(String selectId) {
        List<String> labels = new ArrayList<>();
        labels.add("Default / generated");
        for (Skin skin : skins) labels.add(skin.toString());
        skinSpinner.setAdapter(new ArrayAdapter<>(this,
                android.R.layout.simple_spinner_dropdown_item, labels));
        if (selectId != null) {
            for (int i = 0; i < skins.size(); i++) {
                if (selectId.equals(skins.get(i).id)) {
                    skinSpinner.setSelection(i + 1);
                    return;
                }
            }
        }
        skinSpinner.setSelection(0);
    }

    private Skin skinAt(int spinnerPosition) {
        int index = spinnerPosition - 1;
        return index >= 0 && index < skins.size() ? skins.get(index) : null;
    }

    private void selectAccount(int index) {
        if (accounts.isEmpty()) return;
        selectedAccount = Math.max(0, Math.min(index, accounts.size() - 1));
        Account account = accounts.get(selectedAccount);
        if (name != null) name.setText(account.name);
        if (lookup != null) lookup.setText(account.name);
        if (skinSpinner != null) refreshSkinSpinner(account.skin);
    }

    private void newProfile() {
        String base = "Player";
        int n = 1;
        String candidate;
        do { candidate = base + n++; } while (containsName(candidate, -1));
        accounts.add(new Account(candidate, null));
        selectedAccount = accounts.size() - 1;
        refreshAccountSpinner();
        accountSpinner.setSelection(selectedAccount);
        selectAccount(selectedAccount);
        status("New local profile. Edit its name, then Save / Select.");
    }

    private boolean containsName(String value, int except) {
        for (int i = 0; i < accounts.size(); i++) {
            if (i != except && accounts.get(i).name.equalsIgnoreCase(value)) return true;
        }
        return false;
    }

    private void deleteCurrent() {
        if (accounts.size() <= 1) {
            status("Keep at least one offline profile.");
            return;
        }
        accounts.remove(selectedAccount);
        selectedAccount = Math.min(selectedAccount, accounts.size() - 1);
        try {
            persist();
            refreshAccountSpinner();
            accountSpinner.setSelection(selectedAccount);
            selectAccount(selectedAccount);
            status("Profile deleted.");
        } catch (Exception e) {
            status("Could not delete profile: " + e.getMessage());
        }
    }

    private void saveCurrent() {
        String value = name.getText().toString().trim();
        if (!validName(value)) {
            status("Use 3–16 letters, numbers, or underscores.");
            return;
        }
        if (containsName(value, selectedAccount)) {
            status("That offline profile already exists.");
            return;
        }
        Account account = accounts.get(selectedAccount);
        account.name = value;
        Skin selected = skinAt(skinSpinner.getSelectedItemPosition());
        account.skin = selected == null ? null : selected.id;
        if (selected != null && selected.slim != slim.isChecked()) {
            selected.slim = slim.isChecked();
            writeSkinMeta(selected);
        }
        try {
            persist();
            syncActiveSkin(account);
            refreshAccountSpinner();
            accountSpinner.setSelection(selectedAccount);
            status("Profile saved. New worlds/servers use " + value + ".");
        } catch (Exception e) {
            status("Could not save profile: " + e.getMessage());
        }
    }

    private void loadAccounts() {
        File file = new File(getFilesDir(), ACCOUNTS);
        String selected = null;
        try {
            if (file.isFile()) {
                JSONObject root = new JSONObject(readText(file));
                selected = root.optString("selected", null);
                JSONArray array = root.optJSONArray("accounts");
                if (array != null) {
                    for (int i = 0; i < array.length(); i++) {
                        JSONObject object = array.getJSONObject(i);
                        String n = object.optString("name", "");
                        if (validName(n) && !containsName(n, -1)) {
                            String skin = object.optString("skin", null);
                            if (skin != null && skin.isEmpty()) skin = null;
                            accounts.add(new Account(n, skin));
                        }
                    }
                }
            }
        } catch (Exception ignored) {}
        if (accounts.isEmpty()) {
            String legacy = null;
            try {
                File old = new File(getFilesDir(), LEGACY);
                if (old.isFile()) {
                    for (String line : readText(old).split("\\r?\\n")) {
                        if (line.startsWith("name:")) legacy = line.substring(5);
                    }
                }
            } catch (Exception ignored) {}
            if (!validName(legacy)) legacy = "Player" + String.format(Locale.ROOT, "%05d", System.currentTimeMillis() % 100000);
            accounts.add(new Account(legacy, null));
            selected = legacy;
        }
        selectedAccount = 0;
        if (selected != null) {
            for (int i = 0; i < accounts.size(); i++) {
                if (accounts.get(i).name.equalsIgnoreCase(selected)) selectedAccount = i;
            }
        }
    }

    private void loadSkins() {
        File dir = new File(getFilesDir(), SKIN_DIR);
        File[] metadata = dir.listFiles((d, n) -> n.endsWith(".json"));
        if (metadata == null) return;
        for (File file : metadata) {
            try {
                JSONObject object = new JSONObject(readText(file));
                String id = object.getString("id");
                if (!new File(dir, id + ".png").isFile()) continue;
                skins.add(new Skin(id, object.optString("label", id),
                        "slim".equals(object.optString("model", "classic"))));
            } catch (Exception ignored) {}
        }
        skins.sort(Comparator.comparing(s -> s.label.toLowerCase(Locale.ROOT)));
    }

    private void persist() throws Exception {
        JSONObject root = new JSONObject();
        root.put("version", 1);
        Account selected = accounts.get(selectedAccount);
        root.put("selected", selected.name);
        JSONArray array = new JSONArray();
        for (Account account : accounts) {
            JSONObject object = new JSONObject();
            object.put("name", account.name);
            if (account.skin != null) object.put("skin", account.skin);
            array.put(object);
        }
        root.put("accounts", array);
        atomicWrite(new File(getFilesDir(), ACCOUNTS), root.toString(2).getBytes(StandardCharsets.UTF_8));
        atomicWrite(new File(getFilesDir(), LEGACY), ("name:" + selected.name + "\n").getBytes(StandardCharsets.UTF_8));
    }

    private void syncActiveSkin(Account account) throws Exception {
        File active = new File(getFilesDir(), ACTIVE_SKIN);
        File model = new File(getFilesDir(), ACTIVE_MODEL);
        Skin skin = findSkin(account.skin);
        if (skin == null) {
            active.delete();
            model.delete();
            return;
        }
        File source = new File(new File(getFilesDir(), SKIN_DIR), skin.id + ".png");
        Files.copy(source.toPath(), active.toPath(), StandardCopyOption.REPLACE_EXISTING);
        atomicWrite(model, (skin.slim ? "slim\n" : "classic\n").getBytes(StandardCharsets.UTF_8));
    }

    private Skin findSkin(String id) {
        if (id == null) return null;
        for (Skin skin : skins) if (id.equals(skin.id)) return skin;
        return null;
    }

    private void pickSkin() {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("image/png");
        startActivityForResult(intent, PICK_SKIN);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_SKIN && resultCode == RESULT_OK && data != null && data.getData() != null) {
            try {
                byte[] bytes = readUri(data.getData(), 4 * 1024 * 1024);
                String label = data.getData().getLastPathSegment();
                Skin skin = saveSkin(bytes, label == null ? "Imported skin" : label, slim.isChecked());
                refreshSkinSpinner(skin.id);
                status("Skin imported and saved in Horizon.");
            } catch (Exception e) {
                status("Could not import skin: " + e.getMessage());
            }
        }
    }

    private Skin saveSkin(byte[] bytes, String label, boolean slimModel) throws Exception {
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
        if (options.outWidth != 64 || (options.outHeight != 64 && options.outHeight != 32)) {
            throw new IOException("Minecraft skins must be 64x64 or legacy 64x32 PNGs");
        }
        String id = hex(MessageDigest.getInstance("SHA-256").digest(bytes)).substring(0, 24);
        File dir = new File(getFilesDir(), SKIN_DIR);
        if (!dir.isDirectory() && !dir.mkdirs()) throw new IOException("cannot create skin library");
        atomicWrite(new File(dir, id + ".png"), bytes);
        Skin existing = findSkin(id);
        Skin skin = existing == null ? new Skin(id, label, slimModel) : existing;
        skin.label = label;
        skin.slim = slimModel;
        writeSkinMeta(skin);
        if (existing == null) skins.add(skin);
        skins.sort(Comparator.comparing(s -> s.label.toLowerCase(Locale.ROOT)));
        return skin;
    }

    private void writeSkinMeta(Skin skin) {
        try {
            JSONObject object = new JSONObject();
            object.put("id", skin.id);
            object.put("label", skin.label);
            object.put("model", skin.slim ? "slim" : "classic");
            atomicWrite(new File(new File(getFilesDir(), SKIN_DIR), skin.id + ".json"),
                    object.toString(2).getBytes(StandardCharsets.UTF_8));
        } catch (Exception e) {
            status("Could not update skin metadata: " + e.getMessage());
        }
    }

    private void downloadUsernameSkin() {
        String username = lookup.getText().toString().trim();
        if (!validName(username)) {
            status("Enter a valid Minecraft username first.");
            return;
        }
        status("Looking up " + username + " from Mojang…");
        network.submit(() -> {
            try {
                JSONObject identity = new JSONObject(getText(
                        "https://api.mojang.com/users/profiles/minecraft/" +
                                URLEncoder.encode(username, StandardCharsets.UTF_8.name())));
                String id = identity.getString("id");
                JSONObject profile = new JSONObject(getText(
                        "https://sessionserver.mojang.com/session/minecraft/profile/" + id + "?unsigned=false"));
                JSONArray properties = profile.getJSONArray("properties");
                JSONObject textures = null;
                for (int i = 0; i < properties.length(); i++) {
                    JSONObject p = properties.getJSONObject(i);
                    if ("textures".equals(p.optString("name"))) {
                        textures = new JSONObject(new String(Base64.decode(p.getString("value"), Base64.DEFAULT), StandardCharsets.UTF_8));
                        break;
                    }
                }
                if (textures == null) throw new IOException("Mojang profile has no textures property");
                JSONObject skinObject = textures.getJSONObject("textures").getJSONObject("SKIN");
                String url = skinObject.getString("url");
                URI uri = URI.create(url);
                if (!"https".equalsIgnoreCase(uri.getScheme()) || !"textures.minecraft.net".equalsIgnoreCase(uri.getHost())) {
                    throw new IOException("Mojang returned a non-official skin URL");
                }
                boolean modelSlim = "slim".equals(skinObject.optJSONObject("metadata") == null
                        ? "" : skinObject.getJSONObject("metadata").optString("model", ""));
                byte[] bytes = getBytes(url, 4 * 1024 * 1024);
                Skin skin = saveSkin(bytes, "Mojang: " + profile.optString("name", username), modelSlim);
                runOnUiThread(() -> {
                    refreshSkinSpinner(skin.id);
                    slim.setChecked(skin.slim);
                    status("Downloaded the official Mojang skin for " + username + ". Tap Save / Select.");
                });
            } catch (Exception e) {
                runOnUiThread(() -> status("Could not download skin: " + e.getMessage()));
            }
        });
    }

    private String getText(String url) throws Exception {
        return new String(getBytes(url, 2 * 1024 * 1024), StandardCharsets.UTF_8);
    }

    private byte[] getBytes(String url, int limit) throws Exception {
        URI uri = URI.create(url);
        String host = uri.getHost();
        if (!"https".equalsIgnoreCase(uri.getScheme()) || host == null || !(
                host.equalsIgnoreCase("api.mojang.com")
                        || host.equalsIgnoreCase("sessionserver.mojang.com")
                        || host.equalsIgnoreCase("textures.minecraft.net"))) {
            throw new IOException("not an official Mojang HTTPS endpoint");
        }
        HttpURLConnection connection = (HttpURLConnection) uri.toURL().openConnection();
        connection.setConnectTimeout(15000);
        connection.setReadTimeout(30000);
        connection.setInstanceFollowRedirects(false);
        connection.setRequestProperty("User-Agent", "Horizon-Android/0.1");
        int code = connection.getResponseCode();
        if (code < 200 || code >= 300) throw new IOException("Mojang returned HTTP " + code);
        try (InputStream input = connection.getInputStream(); ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[16384];
            int total = 0;
            int n;
            while ((n = input.read(buffer)) >= 0) {
                total += n;
                if (total > limit) throw new IOException("download is too large");
                output.write(buffer, 0, n);
            }
            return output.toByteArray();
        } finally {
            connection.disconnect();
        }
    }

    private byte[] readUri(Uri uri, int limit) throws Exception {
        try (InputStream input = getContentResolver().openInputStream(uri);
             ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            if (input == null) throw new IOException("file could not be opened");
            byte[] buffer = new byte[16384];
            int total = 0;
            int n;
            while ((n = input.read(buffer)) >= 0) {
                total += n;
                if (total > limit) throw new IOException("skin file is too large");
                output.write(buffer, 0, n);
            }
            return output.toByteArray();
        }
    }

    private static String readText(File file) throws IOException {
        return new String(Files.readAllBytes(file.toPath()), StandardCharsets.UTF_8);
    }

    private static void atomicWrite(File file, byte[] bytes) throws IOException {
        File parent = file.getParentFile();
        if (parent != null && !parent.isDirectory() && !parent.mkdirs()) throw new IOException("cannot create " + parent);
        File temp = new File(file.getPath() + ".tmp");
        try (FileOutputStream out = new FileOutputStream(temp)) {
            out.write(bytes);
            out.getFD().sync();
        }
        try {
            Files.move(temp.toPath(), file.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        } catch (java.nio.file.AtomicMoveNotSupportedException ignored) {
            Files.move(temp.toPath(), file.toPath(), StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static String hex(byte[] bytes) {
        StringBuilder out = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) out.append(String.format(Locale.ROOT, "%02x", b & 0xff));
        return out.toString();
    }

    private void status(String message) {
        if (status != null) status.setText(message);
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show();
    }
}
