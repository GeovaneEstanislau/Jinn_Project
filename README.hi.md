# Jinn OS — Rust में निर्मित न्यूनतम एवं प्रीएम्प्टिव x86_64 माइक्रोकर्नल

<div align="center">

| Language | Status | File | Language | Status | File |
| :--- | :---: | :---: | :--- | :---: | :---: |
| 🇧🇷 **Português** | 🟢 Concluído | [README.md](README.md) | 🇮🇳 **Hindi (हिन्दी)** | 🟢 Concluído | [README.hi.md](README.hi.md) |
| 🇺🇸 **English** | 🟢 Concluído | [README.en.md](README.en.md) | 🇮🇹 **Italiano** | 🟢 Concluído | [README.it.md](README.it.md) |
| 🇨🇳 **中文 (Chinese)** | 🟢 Concluído | [README.zh.md](README.zh.md) | 🇭🇺 **Magyar (Hungarian)**| 🟢 Concluído | [README.hu.md](README.hu.md) |
| 🇯🇵 **日本語 (Japanese)** | 🟢 Concluído | [README.ja.md](README.ja.md) | 🇭🇷 **Hrvatski (Croatian)** | 🟢 Concluído | [README.hr.md](README.hr.md) |
| 🇪🇸 **Español** | 🟢 Concluído | [README.es.md](README.es.md) | 🇳🇴 **Norsk (Norwegian)** | 🟢 Concluído | [README.no.md](README.no.md) |
| 🇷🇺 **Русский (Russian)**| 🟢 Concluído | [README.ru.md](README.ru.md) | 🇩🇪 **Deutsch (German)** | 🟢 Concluído | [README.de.md](README.de.md) |

</div>

> **स्थिति:** सक्रिय विकास में (Alpha v0.0.3)  
> **आर्किटेक्चर:** `x86_64` | **प्रोग्रामिंग भाषा:** Rust (`#![no_std]`, बेयर-मेटल, लक्ष्य `x86_64-unknown-none`)  
> **बूटलोडर:** Limine Boot Protocol v2 (Higher-Half)  
> **समुदाय:** ब्राज़ील में विकसित — कंप्यूटेशनल संप्रभुता, बहुस्तरीय सुरक्षा और न्यूनतम जटिलता के प्रति समर्पित।

---

## परियोजना का परिचय

**Jinn OS** आधुनिक Rust भाषा में पूर्णतः आरंभ से विकसित एक स्वतंत्र माइक्रोकर्नल (Microkernel) ऑपरेटिंग सिस्टम है। इस परियोजना की नींव एक विश्वसनीय, पारदर्शी और औपचारिक रूप से सत्यापन योग्य कंप्यूटिंग आधार तैयार करने के उद्देश्य से रखी गई थी, जहां केवल सबसे आवश्यक बुनियादी घटक ही सीपीयू के सर्वाधिक विशेषाधिकार प्राप्त स्तर (**Ring 0**) में निष्पादित होते हैं।

पारंपरिक मोनोलिथिक कर्नेल के विपरीत, Jinn OS के अंतर्गत सभी जटिल उप-प्रणालियां — जैसे नेटवर्क ड्राइवर, फ़ाइल सिस्टम, **Aladdin Desktop** ग्राफ़िकल विंडो सर्वर और **Jinn Browser** जैसे एप्लिकेशन — यूज़र स्पेस (**Ring 3**) में स्वतंत्र और पृथक प्रक्रियाओं के रूप में चलते हैं। सिस्टम की अखंडता और सीमाओं की सुरक्षा हार्डवेयर-स्तरीय पेजिंग, कठोर अनुमतियों और उच्च-प्रदर्शन युक्त लॉक-फ्री इंटर-प्रोसेस कम्युनिकेशन (IPC) द्वारा सुनिश्चित की जाती है।

---

## मुख्य वास्तुकला स्तंभ और उपलब्धियां (Alpha v0.0.3)

1. **वास्तविक प्रीएम्प्टिव शेड्यूलिंग और मल्टीटास्किंग:**
   - हार्डवेयर PIT टाइमर (100 Hz / 10ms समय क्वांटम) और स्थानीय APIC टाइमर पर आधारित राउंड-रॉबिन प्रीएम्प्टिव शेड्यूलर।
   - इंटरप्ट प्रबंधन (IRQ) और LAPIC EOI (End-Of-Interrupt) का पूर्ण पृथक्करण, जो इंटरप्ट बस के अवरोध को रोकता है।

2. **सख्त मेमोरी अलगाव (x86_64 PML4 एवं W^X सुरक्षा):**
   - 4-स्तरीय पेज टेबल (PML4) के माध्यम से कैनोनिकल वर्चुअल एड्रेस स्पेस का कठोर नियंत्रण।
   - Ring 3 प्रक्रियाओं को सुरक्षित पतों (`>= 0x400000`) पर मैप किया गया है, जिससे फिजिकल मेमोरी अथवा Ring 0 संरचनाओं तक अनधिकृत पहुंच पूर्णतः वर्जित है।
   - सभी गतिशील रूप से आवंटित पृष्ठों पर W^X (*Write XOR Execute*) सुरक्षा नीति का अनिवार्य अनुपालन।

3. **न्यूनतम विशेषाधिकार और कैनोनिकल सिस्कॉल इंटरफ़ेस:**
   - Ring 0 केवल कॉन्टेक्स्ट स्विचिंग, फिजिकल फ़्रेम आवंटन, हार्डवेयर इंटरप्ट वितरण और कैनोनिकल सिस्टम कॉल (Syscalls `0..85` via `syscall`/`sysret` एवं संगत `int 0x80`) तक ही सीमित है।
   - माइक्रोकर्नल कभी भी निजी कुंजियों, निमोनिक सीड या वित्तीय लेन-देन डेटा को स्टोर या प्रोसेस नहीं करता है।

4. **लॉक-फ्री इंटर-प्रोसेस कम्युनिकेशन (Lock-Free IPC):**
   - प्रत्येक प्रक्रिया के लिए स्थिर रूप से बाउंडेड संदेश कतारें, जो महत्वपूर्ण निष्पादन पथ पर किसी भी डायनामिक हीप आवंटन से मुक्त हैं।

5. **ट्रांज़ैक्शनल दृढ़ता के साथ वर्चुअल फ़ाइल सिस्टम (VFS):**
   - **FAT32** (`/disk`) और **Ext2** फ़ाइल सिस्टम का एक साथ नेटिव समर्थन।
   - डिस्क I/O को तीव्र करने हेतु LRU (Least Recently Used) आधारित समर्पित **BlockCache** सब-सिस्टम।
   - मानक POSIX फ़ाइल एक्सेस फ़्लैग्स (`O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_APPEND`, `O_TRUNC`)।
   - रोटेशनल ट्रांज़ैक्शनल सुरक्षा (`.TMP` -> `.BAK`), जो अचानक बिजली कटने पर भी ब्राउज़र कैश, इतिहास, बुकमार्क और कुकीज़ को डेटा करप्शन से बचाती है।

6. **Ring 3 में Aladdin Desktop ग्राफ़िकल वातावरण:**
   - यूज़र स्पेस में पूर्णतः स्वतंत्र रूप से चलने वाला विंडो कंपोजिटर एवं डिस्प्ले सर्वर (`apps/aladdin-server`)।
   - Limine द्वारा उपलब्ध कराए गए उच्च-रिज़ॉल्यूशन लीनियर फ़्रेमबफ़र (Linear Framebuffer) पर प्रत्यक्ष रेंडरिंग।
   - PS/2 माउस ड्राइवर — डीकपल्ड पैकेट हैंडलिंग, स्मूथ रियल-टाइम कर्सर ट्रैकिंग और संपूर्ण विंडो इवेंट प्रबंधन (`Focus`, `Drag`, `Resize`, `Close`) के साथ।

7. **नेटवर्क प्रोटोकॉल स्टैक, JinnWall SPI फ़ायरवॉल एवं PKI क्रिप्टोग्राफ़ी:**
   - बाहरी C लाइब्रेरी से मुक्त बेयर-मेटल IPv4, TCP, UDP, ARP, DHCP एवं DNS स्टैक (VirtIO-Net और Intel E1000 समर्थित)।
   - स्टेटफुल पैकेट इंस्पेक्शन फ़ायरवॉल (**JinnWall SPI**)।
   - नेटिव **PKI X.509** प्रमाणपत्र सत्यापन और HKDF-SHA256/384 युक्त सुरक्षित **TLS 1.3** (RFC 8446) हैंडशेक।

8. **नेटिव Ring 3 यूज़र-स्पेस ऍप्लिकेशन्स:**
   - **Jinn Browser:** C-निर्भरता से मुक्त नेटिव ग्राफ़िकल ब्राउज़र, जिसमें HTML/CSS लेआउट इंजन, HTTP क्लाइंट और `/disk/browser/` में ट्रांज़ैक्शनल रूप से सुरक्षित कैश, बुकमार्क व इतिहास उपलब्ध है।
   - **Aladdin Shell:** कर्नेल आंतरिक स्थिति और डायग्नोस्टिक्स हेतु इंटरैक्टिव टर्मिनल (`sysinfo`, `vfs`, `net`, `ps`, `mem`)।
   - **Aladdin Wallet:** BIP-39/BIP-44 मानकों पर आधारित नॉन-कस्टोडियल Web3 वॉलेट, जो Ring 3 सैंडबॉक्स में पूर्णतः सुरक्षित रूप से निष्पादित होता है।

---

## रिपॉजिटरी संरचना

```text
jinn/
├── Cargo.toml            # माइक्रोकर्नल रूट वर्कस्पेस कॉन्फ़िगरेशन
├── limine.conf           # Limine v2 बूटलोडर कॉन्फ़िगरेशन
├── build.rs              # कर्नेल बिल्ड एवं लिंकिंग स्क्रिप्ट
├── apps/                 # Ring 3 नेटिव यूज़र-स्पेस ऍप्लिकेशन्स एवं सेवाएं
│   ├── aladdin-server/   # Aladdin Desktop विंडो सर्वर (Ring 3 कंपोज़िटर)
│   └── ola-mundo/        # Jinn Browser (browser.rs) एवं Aladdin Wallet (wallet.rs)
├── sdk/                  # Aladdin SDK एवं Ring 3 क्लाइंट रनटाइम लाइब्रेरी
│   └── aladdin-sdk/      # सिस्कॉल, ग्राफ़िक्स, इवेंट्स एवं IPC रैपर्स
├── contracts/            # BEP-20 स्मार्ट कॉन्ट्रैक्ट्स एवं Hardhat टेस्ट सुइट
│   ├── contracts/        # JinnToken.sol, JinnVesting.sol, JinnRewardDistributor.sol
│   ├── test/             # EVM यूनिट टेस्ट्स एवं सुरक्षा इनवेरिएंट्स
│   └── scripts/          # BSC टेस्टनेट एवं मेननेट डिप्लॉयमेंट स्क्रिप्ट्स
├── tools/                # विकास उपयोगिताएँ एवं सिमुलेटर
│   ├── jinn-web3/        # jinn-web3 (CLI) एवं jinn-web3d (Ring 3 डेमॉन)
│   └── jda-sim/          # DAG पाइपलाइन सिमुलेटर एवं SMP बेंचमार्क
├── web/                  # Jinn OS आधिकारिक वेब पोर्टल (jinn-os.dev.br / GitHub Pages)
├── docs/                 # तकनीकी विनिर्देश, ADRs और ऑडिट रिपोर्ट
└── src/                  # माइक्रोकर्नल कोर सोर्स कोड (Ring 0)
    ├── main.rs           # कर्नेल एंट्री पॉइंट (_start) एवं उप-प्रणाली आरंभन
    ├── gdt.rs            # ग्लोबल डिस्क्रिप्टर टेबल (GDT) एवं TSS (Ring 0 / Ring 3)
    ├── idt.rs            # इंटरप्ट डिस्क्रिप्टर टेबल (IDT) एवं ISR/IRQ हैंडलर
    ├── syscall.rs        # कैनोनिकल सिस्टम कॉल डिस्पैचर (Syscalls 0..85)
    ├── scheduler.rs      # प्रीएम्प्टिव मल्टीटास्किंग शेड्यूलर
    ├── ipc.rs            # स्थिर लॉक-फ्री संदेश प्रेषण प्रणाली
    ├── memory/           # फिजिकल फ़्रेम एलोकेटर (Buddy), कर्नेल हीप एवं PML4
    ├── drivers/          # ड्राइवर्स (PIT, LAPIC, कीबोर्ड, PS/2 माउस, IDE, PCI, VirtIO, E1000)
    ├── net/              # TCP/IP स्टैक, JinnWall SPI, TLS 1.3 एवं PKI X.509
    └── vfs/              # वर्चुअल फ़ाइल सिस्टम (FAT32, Ext2, LRU BlockCache)
```

---

## निर्माण और निष्पादन (Build & Run)

### पूर्वापेक्षाएँ
- **Rust टूलचेन:** `x86_64-unknown-none` टारगेट युक्त `nightly` कंपाइलर।
- **एम्यूलेशन उपकरण:** `qemu-system-x86_64`, PowerShell 7+ (Windows/Linux) या Python 3।

```bash
# x86_64 बेयर-मेटल टारगेट जोड़ें
rustup target add x86_64-unknown-none
```

### 1. माइक्रोकर्नल और ऍप्लिकेशन्स का निर्माण
कर्नेल, Ring 3 प्रोग्राम्स और FAT32 बूट डिस्क इमेज को एक साथ संकलित करने हेतु स्वचालित स्क्रिप्ट चलाएं:

```powershell
# रिपॉजिटरी के रूट डायरेक्टरी से चलाएं
.\scripts\build.ps1
```

### 2. QEMU में वर्चुअल मशीन शुरू करें
हार्डवेयर एम्यूलेशन और परिधीय उपकरणों के साथ Jinn OS का परीक्षण करें:

```powershell
# Aladdin Desktop और नेटवर्किंग के साथ मानक निष्पादन
.\scripts\run.ps1

# सीमित मेमोरी के अंतर्गत स्ट्रेस टेस्टिंग (उदा: 64MB RAM सीमा परीक्षण):
.\scripts\run.ps1 -Memory 64M
```

---

## Web3 पारिस्थितिकी तंत्र और Jinn Open Economy ($JINN)

ओपन-सोर्स ऑपरेटिंग सिस्टम अनुसंधान की दीर्घकालिक वित्तीय आत्मनिर्भरता सुनिश्चित करने और *Proof of Useful Contribution* (PoUC) के माध्यम से योगदानकर्ताओं को पुरस्कृत करने हेतु, परियोजना BNB Smart Chain (BSC) पर नेटिव **$JINN** टोकन अवसंरचना को एकीकृत करती है।

> [!IMPORTANT]
> **निजी कुंजियों का पूर्ण अलगाव:** Jinn माइक्रोकर्नल (Ring 0) कभी भी किसी निजी कुंजी, सीड फ़्रेज़ अथवा वित्तीय क्रेडेंशियल्स को एक्सेस या स्टोर नहीं करता। सभी क्रिप्टोग्राफ़िक संचालन यूज़र स्पेस (Ring 3) के भीतर **Aladdin Wallet** एप्लिकेशन में सुरक्षित रूप से पृथक रहते हैं।

- **आर्थिक प्रस्ताव और व्हाइटपेपर:** [`JINN_TOKEN_PROPOSAL.md`](JINN_TOKEN_PROPOSAL.md)
- **EVM सत्यापन और चरण II ऑडिट रिपोर्ट:** [`docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md`](docs/JINN_WEB3_PHASE2_AUDIT_REPORT.md)
- **मल्टी-सिग गवर्नेंस और ट्रेजरी विनिर्देश:** [`docs/JINN_OPEN_ECONOMY_GOVERNANCE.md`](docs/JINN_OPEN_ECONOMY_GOVERNANCE.md)
- **PoUC विनिर्देश (इनाम प्रणाली):** [`docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md`](docs/PROOF_OF_USEFUL_CONTRIBUTION_SPEC.md)
- **वॉलेट आर्किटेक्चर (Ring 3):** [`docs/JINN_WALLET_ARCHITECTURE.md`](docs/JINN_WALLET_ARCHITECTURE.md)
- **Aladdin Wallet यूआई सोर्स कोड:** [`apps/ola-mundo/src/wallet.rs`](apps/ola-mundo/src/wallet.rs)
- **यूज़र-स्पेस डेमॉन `jinn-web3d`:** [`tools/jinn-web3/src/daemon.rs`](tools/jinn-web3/src/daemon.rs)
- **DePIN कंप्यूटेशनल इकोनॉमी (JDA):** [`docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md`](docs/JDA_DEPIN_COMPUTATIONAL_ECONOMY.md)
- **स्मार्ट कॉन्ट्रैक्ट्स आंतरिक ऑडिट:** [`docs/SMART_CONTRACTS_AUDIT_REPORT.md`](docs/SMART_CONTRACTS_AUDIT_REPORT.md)
- **टेस्टनेट डिप्लॉयमेंट गाइड:** [`docs/TESTNET_DEPLOYMENT_GUIDE.md`](docs/TESTNET_DEPLOYMENT_GUIDE.md)

---

## लाइसेंसिंग और शासन

**Jinn OS** दोहरे लाइसेंसिंग मॉडल (**Dual Licensing**) के तहत वितरित किया जाता है:

1. **ओपन कम्युनिटी लाइसेंस (`GPL-2.0-only`):** ओपन-सोर्स डेवलपर्स, शोधकर्ताओं और शौकिया डेवलपर्स के लिए। इसका उपयोग और व्युत्पन्न कार्य GNU General Public License संस्करण 2 के तहत स्वतंत्र हैं ([`LICENSES/GPL-2.0-only.txt`](LICENSES/GPL-2.0-only.txt))।
2. **व्यावसायिक स्वामित्व लाइसेंस (`Jinn-Commercial-1.0`):** औद्योगिक उपकरणों, एम्बेडेड सिस्टम्स और वाणिज्यिक उत्पादों के लिए, जिन्हें GPL कॉपीलेफ्ट शर्तों से छूट की आवश्यकता होती है ([`LICENSES/Jinn-Commercial-1.0.txt`](LICENSES/Jinn-Commercial-1.0.txt))।

पूर्ण कानूनी शर्तों के लिए [`LICENSE`](LICENSE) देखें। कोड योगदान हेतु [Contributor License Agreement (`CLA.md`)](CLA.md) पर हस्ताक्षर और [योगदान दिशानिर्देश (`CONTRIBUTING.md`)](CONTRIBUTING.md) का पालन अनिवार्य है।

---

## संपर्क और योगदान

Jinn OS भारतीय एवं वैश्विक Rust डेवलपर समुदाय, सिस्टम प्रोग्रामर्स और ऑपरेटिंग सिस्टम उत्साही लोगों का हार्दिक स्वागत करता है। चाहे कर्नेल अनुकूलन हो, ड्राइवर लेखन, सुरक्षा ऑडिट या दस्तावेज़ीकरण, आपका हर योगदान अमूल्य है। कृपया पैच और Pull Requests भेजने हेतु [CONTRIBUTING.md](CONTRIBUTING.md) देखें।

