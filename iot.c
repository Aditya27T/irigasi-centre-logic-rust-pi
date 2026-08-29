#include <WiFi.h>
#include <PubSubClient.h>
#include <OneWire.h>
#include <DallasTemperature.h>
#include <ArduinoJson.h>

//=================================================
// PENGATURAN WI-FI & MQTT
//=================================================
const char* ssid        = "Modalo Dewe !!";
const char* password    = "minimalijinsek";
// Broker publik broker.emqx.io -- keputusan tim, dipertahankan (bukan Mosquitto lokal di Pi).
const char* mqtt_server = "broker.emqx.io";
const int   mqtt_port   = 1883;

//=================================================
// IDENTITAS NODE (sesuai skema topic plan.md §2: irigasi/ipesg/v1/<device_id>/...)
//=================================================
const char* device_id   = "esp32-02";
const char* mqtt_id     = device_id;

char topic_sensor[48];
char topic_command[48];

WiFiClient espClient;
PubSubClient client(espClient);

unsigned long lastMsg = 0;
const long interval = 2000;

//=================================================
// PIN SENSOR
//=================================================
#define SOIL3_PIN      34
#define SOIL4_PIN      35

#define DS18B20_3_PIN   4
#define DS18B20_4_PIN  16

//=================================================
// KALIBRASI SOIL MOISTURE (ADC -> Persentase)
//=================================================
const int ADC_KERING = 4095; 
const int ADC_BASAH  = 1000; 

//=================================================
// PIN RELAY 
//=================================================
#define RELAY3 25
#define RELAY4 26

//=================================================
// INSTANSIASI SENSOR DS18B20
//=================================================
OneWire oneWire1(DS18B20_3_PIN);
DallasTemperature sensor3(&oneWire1);

OneWire oneWire2(DS18B20_4_PIN);
DallasTemperature sensor4(&oneWire2);

//=================================================
// FUNGSI KONTROL RELAY
//=================================================
void setRelay(int relayPin, bool status) {
  digitalWrite(relayPin, status ? LOW : HIGH);
}

//=================================================
// STATE COMMAND DARI PI (valve_id 1 -> RELAY3, 2 -> RELAY4)
// Threshold & keputusan buka/tutup TIDAK ada di sini lagi -- semua
// dihitung Pi (decision engine, plan.md §4). ESP32 hanya eksekusi.
//=================================================
struct RelayTimer {
  bool active;
  unsigned long offAt;
};

RelayTimer relayTimer3 = {false, 0};
RelayTimer relayTimer4 = {false, 0};

void applyValveCommand(int valveId, bool open, unsigned long durationMs) {
  int relayPin;
  RelayTimer* timer;

  if (valveId == 1) {
    relayPin = RELAY3;
    timer = &relayTimer3;
  } else if (valveId == 2) {
    relayPin = RELAY4;
    timer = &relayTimer4;
  } else {
    Serial.printf("Command diabaikan: valve_id tidak valid (%d)\n", valveId);
    return;
  }

  setRelay(relayPin, open);
  if (open && durationMs > 0) {
    timer->active = true;
    timer->offAt  = millis() + durationMs;
  } else {
    timer->active = false;
  }
}

// Safety net lokal: matikan relay otomatis begitu duration_s dari command habis,
// supaya valve tidak nyala terus kalau Pi lupa kirim "close" (bukan logika threshold).
void checkRelayTimers() {
  unsigned long now = millis();
  if (relayTimer3.active && now >= relayTimer3.offAt) {
    setRelay(RELAY3, false);
    relayTimer3.active = false;
    Serial.println("Relay 3 auto-off (durasi command habis)");
  }
  if (relayTimer4.active && now >= relayTimer4.offAt) {
    setRelay(RELAY4, false);
    relayTimer4.active = false;
    Serial.println("Relay 4 auto-off (durasi command habis)");
  }
}

// Dipanggil PubSubClient saat ada pesan masuk di topic_command.
// Payload mengikuti skema plan.md §2: {"action":"open"|"close","valve_id":1|2,"duration_s":120,...}
void onMqttMessage(char* topic, byte* payload, unsigned int length) {
  StaticJsonDocument<256> cmd;
  DeserializationError err = deserializeJson(cmd, payload, length);
  if (err) {
    Serial.print("Gagal parse command JSON: ");
    Serial.println(err.c_str());
    return;
  }

  const char* action  = cmd["action"]     | "none";
  int  valveId         = cmd["valve_id"]   | 0;
  long durationS        = cmd["duration_s"] | 0;

  Serial.printf("Command diterima: action=%s valve_id=%d duration_s=%ld\n", action, valveId, durationS);

  if (strcmp(action, "open") == 0) {
    applyValveCommand(valveId, true, (unsigned long)durationS * 1000UL);
  } else if (strcmp(action, "close") == 0) {
    applyValveCommand(valveId, false, 0);
  }
}

//=================================================
// KONEKSI WI-FI
//=================================================
void setupWiFi() {
  delay(10);
  Serial.println();
  Serial.print("Menghubungkan ke Wi-Fi: ");
  Serial.println(ssid);

  WiFi.begin(ssid, password);

  while (WiFi.status() != WL_CONNECTED) {
    delay(500);
    Serial.print(".");
  }

  Serial.println("\nWi-Fi Terhubung!");
  Serial.print("IP Address: ");
  Serial.println(WiFi.localIP());
}

//=================================================
// REKONEKSI MQTT
//=================================================
void reconnect() {
  while (!client.connected()) {
    Serial.print("Mencoba koneksi MQTT...");
    if (client.connect(mqtt_id)) {
      Serial.println("Terhubung ke MQTT Broker!");
      client.subscribe(topic_command);
      Serial.print("Subscribe command: ");
      Serial.println(topic_command);
    } else {
      Serial.print("Gagal, rc=");
      Serial.print(client.state());
      Serial.println(" Coba lagi dalam 5 detik.");
      delay(5000);
    }
  }
}

//=================================================
// FUNGSI PENENTU STATUS TANAH
//=================================================
String getStatusTanah(int persentase) {
  if (persentase >= 60) {
    return "Basah";
  } else if (persentase >= 40) {
    return "Normal";
  } else {
    return "Kering";
  }
}

//=================================================
// SETUP
//=================================================
void setup() {
  Serial.begin(115200);
  analogReadResolution(12);

  sensor3.begin();
  sensor4.begin();

  // Inisialisasi hanya 2 Relay
  pinMode(RELAY3, OUTPUT);
  pinMode(RELAY4, OUTPUT);
  
  // Matikan relay saat pertama kali nyala
  setRelay(RELAY3, false);
  setRelay(RELAY4, false);

  setupWiFi();
  client.setServer(mqtt_server, mqtt_port);
  client.setCallback(onMqttMessage);

  snprintf(topic_sensor, sizeof(topic_sensor), "irigasi/ipesg/v1/%s/sensor", device_id);
  snprintf(topic_command, sizeof(topic_command), "irigasi/ipesg/v1/%s/command", device_id);
  Serial.print("Topic sensor : ");
  Serial.println(topic_sensor);
  Serial.print("Topic command: ");
  Serial.println(topic_command);

  Serial.println("==========================================");
  Serial.println("SMART IRIGATION IPESG v2 (Kontrol dari Pi)");
  Serial.println("==========================================\n");
}

//=================================================
// LOOP UTAMA
//=================================================
void loop() {
  if (!client.connected()) {
    reconnect();
  }
  client.loop();
  checkRelayTimers();

  unsigned long now = millis();
  if (now - lastMsg > interval) {
    lastMsg = now;

    // 1. Baca Nilai Raw ADC & Konversi ke Persentase
    int rawSoil3 = analogRead(SOIL3_PIN);
    int rawSoil4 = analogRead(SOIL4_PIN);

    int soil3_pct = map(rawSoil3, ADC_KERING, ADC_BASAH, 0, 100);
    soil3_pct = constrain(soil3_pct, 0, 100); 

    int soil4_pct = map(rawSoil4, ADC_KERING, ADC_BASAH, 0, 100);
    soil4_pct = constrain(soil4_pct, 0, 100);

    String statusSoil3 = getStatusTanah(soil3_pct);
    String statusSoil4 = getStatusTanah(soil4_pct);

    // 2. Baca Sensor Suhu
    sensor3.requestTemperatures();
    sensor4.requestTemperatures();
    float suhu3 = sensor3.getTempCByIndex(0);
    float suhu4 = sensor4.getTempCByIndex(0);

    // 3. Kontrol relay TIDAK dihitung di sini lagi -- ESP32 hanya menjalankan
    //    command yang diterima dari Pi via onMqttMessage()/applyValveCommand().
    //    Threshold & keputusan buka/tutup sepenuhnya di decision engine Pi (plan.md §4).

    // 4. Tampilkan ke Serial Monitor
    Serial.println("--- DATA SENSOR ---");
    Serial.printf("Soil 3 : %d%% (%s) [Raw: %d]\n", soil3_pct, statusSoil3.c_str(), rawSoil3);
    Serial.printf("Soil 4 : %d%% (%s) [Raw: %d]\n", soil4_pct, statusSoil4.c_str(), rawSoil4);
    Serial.printf("Suhu 3 : %.2f °C \n", suhu3);
    Serial.printf("Suhu 4 : %.2f °C \n", suhu4);
    
    Serial.printf("Relay 3: %s\n", digitalRead(RELAY3) == LOW ? "ON" : "OFF");
    Serial.printf("Relay 4: %s\n", digitalRead(RELAY4) == LOW ? "ON" : "OFF");
    Serial.println("-------------------");

    // 5. Buat Payload JSON dan Publish
    StaticJsonDocument<256> doc;
    doc["device_id"] = device_id;
    doc["soil3_pct"] = soil3_pct;
    doc["status3"]   = statusSoil3;
    doc["soil4_pct"] = soil4_pct;
    doc["status4"]   = statusSoil4;
    doc["suhu3"]     = suhu3;
    doc["suhu4"]     = suhu4;
    
    doc["relay3"]    = digitalRead(RELAY3) == LOW ? "ON" : "OFF";
    doc["relay4"]    = digitalRead(RELAY4) == LOW ? "ON" : "OFF";

    char jsonBuffer[512];
    serializeJson(doc, jsonBuffer);
    
    client.publish(topic_sensor, jsonBuffer);
  }
}