using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Drawing;
using System.Windows.Forms;
using System.Collections.Generic;
using System.Text.RegularExpressions;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Formatters.Soap;

namespace MusicBeePlugin
{
    public partial class Plugin
    {
        private MusicBeeApiInterface mbApiInterface;
        private PluginInfo about = new PluginInfo();

        private string ConfigDirectory {
            get { return mbApiInterface.Setting_GetPersistentStoragePath() + "\\linux-media-controls"; }
        }

        private string ConfigFile {
            get { return this.ConfigDirectory + "\\config.txt"; }
        }

        private const string metadataFile = "metadata";
        private const string playbackFile = "playback";
        private const string activatedFile = "plugin-activated";

        private Config config;
        private ConfigPanel panel;

        public PluginInfo Initialise(IntPtr apiInterfacePtr)
        {
            mbApiInterface = new MusicBeeApiInterface();
            mbApiInterface.Initialise(apiInterfacePtr);
            about.PluginInfoVersion = PluginInfoVersion;
            about.Name = "Linux Media Controls";
            about.Description = "Provides media control support to linux under wine";
            about.Author = "ThatEpicBanana";
            about.TargetApplication = "";   //  the name of a Plugin Storage device or panel header for a dockable panel
            about.Type = PluginType.General;
            about.VersionMajor = 1;  // your plugin version
            about.VersionMinor = 0;
            about.Revision = 1;
            about.MinInterfaceVersion = MinInterfaceVersion;
            about.MinApiRevision = MinApiRevision;
            about.ReceiveNotifications = (ReceiveNotificationFlags.PlayerEvents | ReceiveNotificationFlags.TagEvents);
            about.ConfigurationPanelHeight = 20;   // height in pixels that musicbee should reserve in a panel for config settings. When set, a handle to an empty panel will be passed to the Configure function

            Directory.CreateDirectory(this.ConfigDirectory);
            this.config = this.getConfig();
            this.panel = new ConfigPanel(this.config);

            this.CreateFileStructure();

            return about;
        }

        public bool Configure(IntPtr panelHandle)
        {
            // panelHandle will only be set if you set about.ConfigurationPanelHeight to a non-zero value
            // keep in mind the panel width is scaled according to the font the user has selected
            // if about.ConfigurationPanelHeight is set to 0, you can display your own popup window
            if (panelHandle != IntPtr.Zero)
            {
                this.panel.attach(panelHandle);
            }
            return false;
        }

        // called by MusicBee when the user clicks Apply or Save in the MusicBee Preferences screen.
        // its up to you to figure out whether anything has changed and needs updating
        public void SaveSettings()
        {
            this.config = this.panel.update();
            this.saveConfig(this.config);
        }

        // MusicBee is closing the plugin (plugin is being disabled by user or MusicBee is shutting down)
        public void Close(PluginCloseReason reason)
        {
            this.Deactivate();
        }

        // uninstall this plugin - clean up any persisted files
        public void Uninstall()
        {
            Directory.Delete(this.ConfigDirectory);
        }

        // receive event notifications from MusicBee
        public void ReceiveNotification(string sourceFileUrl, NotificationType type)
        {
            switch (type)
            {
                case NotificationType.PluginStartup:
                    this.UpdatePlayback();
                    this.UpdateMetaData();
                    this.Activate();
                    break;
                case NotificationType.TrackChanged:
                case NotificationType.TagsChanged: // dunno if this does anything but might as well
                    this.UpdateMetaData();
                    break;
                case NotificationType.PlayStateChanged:
                    this.UpdatePlayback();
                    break;
            }
        }

        private void CreateFileStructure()
        {
            Directory.CreateDirectory(this.config.rootDirectory);
            File.Create(this.config.rootDirectory + activatedFile).Close();
            File.Create(this.config.rootDirectory + playbackFile).Close();
            File.Create(this.config.rootDirectory + metadataFile).Close();
        }

        private void Activate()
        {
            File.WriteAllText(this.config.rootDirectory + activatedFile, "true");
        }

        private void Deactivate()
        {
            File.WriteAllText(this.config.rootDirectory + activatedFile, "false");
        }

        private void UpdatePlayback() 
        {
            string state = null;

            switch (mbApiInterface.Player_GetPlayState())
            {
                case PlayState.Paused:
                    state = "paused";
                    break;
                case PlayState.Playing:
                    state = "playing";
                    break;
                default:
                    state = "stopped";
                    break;
            }

            File.WriteAllText(this.config.rootDirectory + playbackFile, state);
        }

        private void UpdateMetaData() 
        {
            string title = mbApiInterface.NowPlaying_GetFileTag(MetaDataType.TrackTitle);
            string album = mbApiInterface.NowPlaying_GetFileTag(MetaDataType.Album);
            string artist = mbApiInterface.NowPlaying_GetFileTag(MetaDataType.Artist);
            string cover = mbApiInterface.NowPlaying_GetArtworkUrl();
            int duration = mbApiInterface.NowPlaying_GetDuration();

            File.WriteAllText(this.config.rootDirectory + metadataFile, 
                title + "\n" +
                album + "\n" +
                artist + "\n" +
                cover + "\n" +
                duration
            );
        }

        private Config getConfig() {
            Config val;

            if(File.Exists(this.ConfigFile)) {
                val = Config.deserialize(File.ReadAllText(this.ConfigFile));
            } else {
                val = Config.def();
                this.saveConfig(val);
            }

            return val;
        }

        private void saveConfig(Config config) {
            File.WriteAllText(this.ConfigFile, config.serialize());
        }
    }

    [Serializable()]
    class Config {
        private static Regex file_regex = new Regex("^([a-zA-Z]\\:)(\\\\[^\\\\/:*?<>\"|]*(?<![ ]))*(\\.[a-zA-Z]{2,6})$", RegexOptions.Compiled);

        public string rootDirectory;

        public static Config def() {
            return new Config() { rootDirectory = "Z:\\\\tmp\\musicbee-mediakeys\\" };
        }

        // checks if some configuration is valid
        // this may be able to fix some issues if invalid
        // returns true if the configuration is valid (including if it has been fixed), and false if it isn't
        public bool validate() {
            return validateRoot();
        }

        private bool validateRoot() {
            string root = this.rootDirectory;
            
            // replace \ with /
            root = root.Replace('/', '\\');
            // if the root starts with /, assume the user meant a linux location
            if(root.StartsWith("\\")) 
                root = "Z:" + root;
            // add a \ at the end if it isn't present 
            if(!root.EndsWith("\\"))
                root = root + '\\';
            // replace c:\ with c:\\
            if(!root.Contains(":\\\\"))
                root = root.Replace(":\\", ":\\\\");

            // check if it's valid
            if(!ReferenceEquals(new FileInfo(root), null)) {
                this.rootDirectory = root;
                return true;
            } else return false;
        }

        // I originally used xml for this, but 
        // the serializer doesn't work with backslashes well
        public string serialize() {
            return this.rootDirectory;
        }

        public static Config deserialize(string val) {
            return new Config() {
                rootDirectory = val,
            };
        }
    }

    class ConfigPanel {
        private Config config;

        private Label rootLabel;
        private TextBox rootBox;

        public ConfigPanel(Config config) {
            this.config = config;
        }

        public Config update() {
            Config config = new Config() {
                rootDirectory = this.rootBox.Text
            };

            // fallback to old config if it's broken
            if(config.validate()) {
                this.config = config;
                this.updatePanel();
                return this.config;
            } else {
                return this.config;
            }
        }

        public void attach(IntPtr panelHandle) {
            Panel panel = (Panel) Panel.FromHandle(panelHandle);

            this.rootLabel = new Label() {
                Text = "Root Directory: ",
                AutoSize = true,
                Location = new Point(0, 0)
            };
            
            this.rootBox = new TextBox() {
                Bounds = new Rectangle(this.rootLabel.Width, 0, 200, this.rootLabel.Height)
            };

            this.updatePanel();

            panel.Controls.AddRange(new Control[] { this.rootLabel, this.rootBox });
        }

        public void updatePanel() {
            this.rootBox.Text = this.config.rootDirectory;
        }
    }
}
